// A cave's LiDAR shots (CaveCAD Capture's `.cvcap` files), kept on the server so every device of the cave's
// people can download them: the big-file sibling of scans.ts. A shot is known by its shot id (from the file's own
// header); its bytes are a file named by their SHA-256 in the captures folder, stored once and deleted when no
// cave's shot uses it. Files are tens of MB, so they come in chunks a phone can resume: each chunk is appended to
// `uploads/<cave>-<sha>.part` only at the part's current end, and the whole file is checked (SHA-256, CVCAP magic,
// header naming this cave) before it is kept. Owners and editors add and remove shots; watchers only download.
import { createHash } from 'node:crypto';
import { createReadStream, existsSync, mkdirSync, openSync, readSync, closeSync, readdirSync, renameSync, rmSync, statSync, appendFileSync } from 'node:fs';
import { join } from 'node:path';
import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Db } from './db.ts';
import { accessOf } from './rooms.ts';
import { send } from './http.ts';

/** The largest shot the server takes. */
export const MAX_CAPTURE_BYTES = 512 * 1024 * 1024;
/** The largest chunk one request may carry. */
export const MAX_CHUNK_BYTES = 8 * 1024 * 1024;
/** A part not finished for this long is thrown away when another upload starts. */
export const STALE_PART_MS = 48 * 3600 * 1000;
/** The largest header JSON read out of a file (the header carries cards and passes, never mesh data). */
const MAX_HEADER_BYTES = 16 * 1024 * 1024;
const MAGIC = Buffer.from('CVCAP\0', 'latin1');
/** A shot id: what Capture writes (a UUID), kept to plain characters so it is safe in a path. */
const SHOT = /^[A-Za-z0-9_.:-]{1,100}$/;
const SHA = /^[0-9a-f]{64}$/;

/** One shot of a cave's captures. */
export type CaptureShot = { shotId: string; sha: string; size: number; stationId: string; stationLabel: string; capturedAt: number; device: string; addedBy: string; addedAt: number };

const fail = (code: string): never => {
  throw new Error(code);
};

function requireReader(db: Db, memberId: string, caveId: string): void {
  if (!accessOf(db, memberId, caveId)) fail('not-found');
}
function requireWriter(db: Db, memberId: string, caveId: string): void {
  const a = accessOf(db, memberId, caveId) ?? fail('not-found');
  if (a === 'watcher') fail('watcher');
}

type Row = { shot_id: string; sha: string; size: number; station_id: string; station_label: string; captured_at: number; device: string; added_by: string; added_at: number };
const toShot = (r: Row): CaptureShot => ({ shotId: r.shot_id, sha: r.sha, size: r.size, stationId: r.station_id, stationLabel: r.station_label, capturedAt: r.captured_at, device: r.device, addedBy: r.added_by, addedAt: r.added_at });

/** The cave's shots, newest first. */
export function listCaptures(db: Db, memberId: string, caveId: string): CaptureShot[] {
  requireReader(db, memberId, caveId);
  return (db.prepare('SELECT * FROM cave_captures WHERE cave_id = ? ORDER BY captured_at DESC, added_at DESC, shot_id').all(caveId) as Row[]).map(toShot);
}

/** What the server takes from a capture's header. */
type Header = { shotId: string; stationId: string; stationLabel: string; capturedAt: number; device: string };

/** Reads and checks a kept or uploaded file's header: CVCAP magic, then a JSON header naming `caveId`. */
export function readHeader(file: string, caveId: string): Header {
  const fd = openSync(file, 'r');
  try {
    const size = statSync(file).size;
    const head = Buffer.alloc(12);
    if (size < 12 || readSync(fd, head, 0, 12, 0) !== 12 || !head.subarray(0, 6).equals(MAGIC)) fail('bad-capture');
    if (head.readUInt16LE(6) < 1) fail('bad-capture');
    const len = head.readUInt32LE(8);
    if (len < 2 || len > MAX_HEADER_BYTES || 12 + len > size) fail('bad-capture');
    const json = Buffer.alloc(len);
    if (readSync(fd, json, 0, len, 12) !== len) fail('bad-capture');
    let wire: unknown;
    try {
      wire = JSON.parse(json.toString('utf8'));
    } catch {
      return fail('bad-capture');
    }
    const h = (wire as { header?: unknown } | null)?.header as Record<string, unknown> | undefined;
    if (!h || typeof h !== 'object') return fail('bad-capture');
    const text = (v: unknown, empty: boolean) => (typeof v === 'string' && (empty || v) ? v.slice(0, 200) : fail('bad-capture'));
    if (typeof h.shot_id !== 'string' || !SHOT.test(h.shot_id)) fail('bad-capture');
    if (h.cave_id !== caveId) fail('wrong-cave');
    const at = h.captured_at_ms;
    if (typeof at !== 'number' || !Number.isFinite(at)) fail('bad-capture');
    return { shotId: h.shot_id as string, stationId: text(h.station_id ?? '', true), stationLabel: text(h.station_label ?? '', true), capturedAt: Math.trunc(at as number), device: text(h.device ?? '', true) };
  } finally {
    closeSync(fd);
  }
}

/** Keeps a shot whose checked file is at `<dir>/<sha>`: its row replaces the same shot's, and the old file goes if unused. */
function keep(db: Db, dir: string, memberId: string, caveId: string, sha: string, now: number): CaptureShot {
  const file = join(dir, sha);
  let h: Header;
  try {
    h = readHeader(file, caveId);
  } catch (e) {
    dropUnused(db, dir, [sha]); // a file nothing uses never stays
    throw e;
  }
  const size = statSync(file).size;
  const before = db.prepare('SELECT sha FROM cave_captures WHERE cave_id = ? AND shot_id = ?').get(caveId, h.shotId) as { sha: string } | undefined;
  db.prepare(
    `INSERT INTO cave_captures (cave_id, shot_id, sha, size, station_id, station_label, captured_at, device, added_by, added_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
     ON CONFLICT(cave_id, shot_id) DO UPDATE SET sha = excluded.sha, size = excluded.size, station_id = excluded.station_id, station_label = excluded.station_label,
       captured_at = excluded.captured_at, device = excluded.device, added_by = excluded.added_by, added_at = excluded.added_at`,
  ).run(caveId, h.shotId, sha, size, h.stationId, h.stationLabel, h.capturedAt, h.device, memberId, now);
  if (before && before.sha !== sha) dropUnused(db, dir, [before.sha]);
  return { shotId: h.shotId, sha, size, stationId: h.stationId, stationLabel: h.stationLabel, capturedAt: h.capturedAt, device: h.device, addedBy: memberId, addedAt: now };
}

const partOf = (dir: string, caveId: string, sha: string) => join(dir, 'uploads', `${caveId}-${sha}.part`);
const sizeOf = (file: string) => {
  try {
    return statSync(file).size;
  } catch {
    return 0;
  }
};

/** How much of an upload the server has (writers only). */
export function uploadHave(db: Db, dir: string, memberId: string, caveId: string, sha: string): number {
  requireWriter(db, memberId, caveId);
  if (!SHA.test(sha)) fail('not-found');
  return sizeOf(partOf(dir, caveId, sha));
}

/** Deletes upload parts untouched for STALE_PART_MS. */
export function sweepStaleParts(dir: string, now: number): void {
  const up = join(dir, 'uploads');
  let names: string[];
  try {
    names = readdirSync(up);
  } catch {
    return;
  }
  for (const n of names) {
    try {
      if (n.endsWith('.part') && now - statSync(join(up, n)).mtimeMs > STALE_PART_MS) rmSync(join(up, n), { force: true });
    } catch {
      // gone already
    }
  }
}

const int = (v: string | null) => (v !== null && /^\d{1,12}$/.test(v) ? Number(v) : fail('bad-request'));

/** SHA-256 of a file, read as a stream (files run to hundreds of MB). */
async function hashFile(file: string): Promise<string> {
  const h = createHash('sha256');
  for await (const chunk of createReadStream(file)) h.update(chunk as Buffer);
  return h.digest('hex');
}

/** Reads one chunk's bytes, refusing more than `max`. */
async function readChunk(req: IncomingMessage, max: number): Promise<Buffer> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of req) {
    const b = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    size += b.length;
    if (size > max) fail('too-big');
    chunks.push(b);
  }
  return Buffer.concat(chunks);
}

type Answer = { have: number } | { capture: CaptureShot } | { offset: number };

/**
 * One chunk of an upload (writers only): appended when `offset` is the part's current size, else `{offset: have}`
 * (answered 409). When the part reaches `total` the whole file is checked and kept. If the server already holds a
 * file with this SHA-256, the upload finishes at once from it, whatever the offset.
 */
export async function putChunk(db: Db, dir: string, memberId: string, caveId: string, sha: string, offsetRaw: string | null, totalRaw: string | null, req: IncomingMessage, now: number): Promise<Answer> {
  requireWriter(db, memberId, caveId);
  if (!SHA.test(sha)) fail('not-found');
  const offset = int(offsetRaw);
  const total = int(totalRaw);
  if (total > MAX_CAPTURE_BYTES) fail('too-big');
  if (total < 1 || offset >= total) fail('bad-request');
  const declared = req.headers['content-length'] === undefined ? undefined : Number(req.headers['content-length']);
  if (declared !== undefined && declared > MAX_CHUNK_BYTES) fail('too-big');
  if (declared !== undefined && offset + declared > total) fail('bad-request');
  if (offset === 0) sweepStaleParts(dir, now);
  if (existsSync(join(dir, sha))) {
    req.resume();
    return { capture: keep(db, dir, memberId, caveId, sha, now) };
  }
  const part = partOf(dir, caveId, sha);
  if (sizeOf(part) !== offset) return { offset: sizeOf(part) };
  const bytes = await readChunk(req, MAX_CHUNK_BYTES);
  if (!bytes.length || offset + bytes.length > total) fail('bad-request');
  // checked again after the body arrived, then appended in the same tick: two racing chunks cannot both land
  if (sizeOf(part) !== offset) return { offset: sizeOf(part) };
  mkdirSync(join(dir, 'uploads'), { recursive: true });
  appendFileSync(part, bytes);
  const have = offset + bytes.length;
  if (have < total) return { have };
  if ((await hashFile(part)) !== sha) {
    rmSync(part, { force: true });
    fail('bad-sha');
  }
  try {
    readHeader(part, caveId);
  } catch (e) {
    rmSync(part, { force: true });
    throw e;
  }
  const file = join(dir, sha);
  if (existsSync(file)) rmSync(part, { force: true });
  else renameSync(part, file); // whole or not at all
  return { capture: keep(db, dir, memberId, caveId, sha, now) };
}

/** Takes a shot off the cave (writers only). */
export function removeCapture(db: Db, dir: string, memberId: string, caveId: string, shotId: string): void {
  requireWriter(db, memberId, caveId);
  const row = db.prepare('SELECT sha FROM cave_captures WHERE cave_id = ? AND shot_id = ?').get(caveId, shotId) as { sha: string } | undefined;
  if (!row) fail('not-found');
  db.prepare('DELETE FROM cave_captures WHERE cave_id = ? AND shot_id = ?').run(caveId, shotId);
  dropUnused(db, dir, [row!.sha]);
}

/** Deletes every kept file no shot uses, and a deleted cave's unfinished parts. */
export function sweepCaptures(db: Db, dir: string, caveId?: string): void {
  let names: string[];
  try {
    names = readdirSync(dir);
  } catch {
    return; // nothing kept yet
  }
  dropUnused(db, dir, names.filter((n) => SHA.test(n)));
  if (!caveId) return;
  try {
    for (const n of readdirSync(join(dir, 'uploads'))) if (n.startsWith(`${caveId}-`) && n.endsWith('.part')) rmSync(join(dir, 'uploads', n), { force: true });
  } catch {
    // no uploads folder
  }
}

/** Deletes the files of `shas` that no shot uses any more. */
export function dropUnused(db: Db, dir: string, shas: string[]): void {
  for (const sha of shas) {
    if (db.prepare('SELECT 1 FROM cave_captures WHERE sha = ? LIMIT 1').get(sha)) continue;
    rmSync(join(dir, sha), { force: true });
  }
}

/** Sends a kept file (readers), whole or one `Range: bytes=a-b` of it. */
function sendFile(db: Db, dir: string, memberId: string, caveId: string, sha: string, req: IncomingMessage, res: ServerResponse): void {
  requireReader(db, memberId, caveId);
  if (!db.prepare('SELECT 1 FROM cave_captures WHERE cave_id = ? AND sha = ? LIMIT 1').get(caveId, sha)) fail('not-found');
  const file = join(dir, sha);
  const size = sizeOf(file);
  if (!size) fail('not-found');
  const head = { 'Content-Type': 'application/octet-stream', 'Accept-Ranges': 'bytes', 'Cache-Control': 'private, max-age=31536000, immutable' };
  const range = String(req.headers.range ?? '');
  let start = 0;
  let end = size - 1;
  let partial = false;
  if (range) {
    const m = /^bytes=(\d*)-(\d*)$/.exec(range.trim());
    if (m && (m[1] || m[2])) {
      if (m[1]) {
        start = Number(m[1]);
        end = m[2] ? Math.min(Number(m[2]), size - 1) : size - 1;
      } else {
        start = Math.max(0, size - Number(m[2]));
      }
      if (start >= size || start > end) {
        res.writeHead(416, { ...head, 'Content-Range': `bytes */${size}` });
        return void res.end();
      }
      partial = true;
    } // a range we do not understand: the whole file, as HTTP allows
  }
  res.writeHead(partial ? 206 : 200, { ...head, 'Content-Length': end - start + 1, ...(partial ? { 'Content-Range': `bytes ${start}-${end}/${size}` } : {}) });
  if (req.method === 'HEAD') return void res.end();
  const stream = createReadStream(file, { start, end });
  stream.on('error', () => res.destroy());
  stream.pipe(res);
}

const STATUS: Record<string, number> = { 'not-found': 404, watcher: 403, 'bad-request': 400, 'too-big': 413, 'bad-sha': 422, 'bad-capture': 422, 'wrong-cave': 422 };

/**
 * `/api/caves/:cave/captures`: `GET` lists the shots. `PUT .../:sha?offset=&total=` sends one chunk;
 * `GET`/`HEAD .../:sha/upload` says how much has arrived. `GET .../:sha` gives the bytes (with ranges).
 * `DELETE .../:shotId` takes a shot off. See CAPTURES.md.
 */
export async function handleCaptures(db: Db, dir: string, memberId: string, req: IncomingMessage, res: ServerResponse, caveId: string, item: string, upload: boolean, now: number): Promise<void> {
  const url = new URL(req.url ?? '/', 'http://x');
  const method = req.method ?? 'GET';
  try {
    if (!item) {
      if (method === 'GET') return send(res, 200, { captures: listCaptures(db, memberId, caveId) });
      return send(res, 404, { error: 'not-found' });
    }
    if (upload) {
      if (method !== 'GET' && method !== 'HEAD') return send(res, 404, { error: 'not-found' });
      const have = uploadHave(db, dir, memberId, caveId, item);
      res.setHeader('Upload-Offset', String(have));
      return send(res, 200, { have });
    }
    if (method === 'DELETE') {
      if (!SHOT.test(item)) fail('not-found');
      removeCapture(db, dir, memberId, caveId, item);
      return send(res, 200, { ok: true });
    }
    if (!SHA.test(item)) return send(res, 404, { error: 'not-found' });
    if (method === 'GET' || method === 'HEAD') return sendFile(db, dir, memberId, caveId, item, req, res);
    if (method === 'PUT') {
      const got = await putChunk(db, dir, memberId, caveId, item, url.searchParams.get('offset'), url.searchParams.get('total'), req, now);
      if ('offset' in got) {
        req.resume();
        return send(res, 409, { error: 'offset', have: got.offset });
      }
      return send(res, 200, got);
    }
    return send(res, 404, { error: 'not-found' });
  } catch (e) {
    const code = e instanceof Error ? e.message : '';
    if (STATUS[code]) {
      req.resume(); // let any unread body drain so the answer arrives
      return send(res, STATUS[code], { error: code });
    }
    throw e;
  }
}
