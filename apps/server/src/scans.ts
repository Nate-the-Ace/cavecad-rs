// A cave's scanned pages, kept on the server so every device of the owner and everyone the cave is shared with
// sees the same pages. A page is known by its path in the cave's scans (the path trips link it by); its bytes
// are a file named by their SHA-256 in the scans folder, so the same picture added twice is stored once and is
// deleted only when no cave's page uses it. Owners and editors add and remove pages; watchers only look.
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Db } from './db.ts';
import { accessOf } from './rooms.ts';
import { send } from './http.ts';

/** The largest page the server takes (a big scan, or a PDF of a few pages). */
export const MAX_SCAN_BYTES = 25 * 1024 * 1024;
/** The kinds of file a page can be, by extension. */
const TYPES: Record<string, string> = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', webp: 'image/webp', gif: 'image/gif', tif: 'image/tiff', tiff: 'image/tiff', pdf: 'application/pdf' };

/** One page of a cave's scans. */
export type ScanPage = { path: string; sha: string; size: number; type: string; addedAt: number };

const fail = (code: string): never => {
  throw new Error(code);
};

/** A page's path: relative, folders by `/`, no `.` or `..` parts, at most 300 characters, ending in a scan's extension. */
export function scanPath(p: unknown): string {
  if (typeof p !== 'string') return fail('bad-path');
  const s = p.normalize('NFC');
  if (!s || s.length > 300 || s.startsWith('/') || s.includes('\\') || s.includes('\0')) fail('bad-path');
  if (s.split('/').some((part) => !part || part === '.' || part === '..')) fail('bad-path');
  if (!TYPES[s.slice(s.lastIndexOf('.') + 1).toLowerCase()]) fail('bad-type');
  return s;
}
const typeOf = (path: string) => TYPES[path.slice(path.lastIndexOf('.') + 1).toLowerCase()];

function requireReader(db: Db, memberId: string, caveId: string): void {
  if (!accessOf(db, memberId, caveId)) fail('not-found');
}
function requireWriter(db: Db, memberId: string, caveId: string): void {
  const a = accessOf(db, memberId, caveId) ?? fail('not-found');
  if (a === 'watcher') fail('watcher');
}

/** The cave's pages, by path. */
export function listScans(db: Db, memberId: string, caveId: string): ScanPage[] {
  requireReader(db, memberId, caveId);
  return (
    db.prepare('SELECT path, sha, size, type, added_at FROM cave_scans WHERE cave_id = ? ORDER BY path').all(caveId) as { path: string; sha: string; size: number; type: string; added_at: number }[]
  ).map((r) => ({ path: r.path, sha: r.sha, size: r.size, type: r.type, addedAt: r.added_at }));
}

/** Keeps a page's bytes (writers only); a page already at the path is replaced. Gives back what was kept. */
export function putScan(db: Db, dir: string, memberId: string, caveId: string, rawPath: unknown, bytes: Buffer, now: number): ScanPage {
  requireWriter(db, memberId, caveId);
  const path = scanPath(rawPath);
  if (!bytes.length) fail('bad-request');
  if (bytes.length > MAX_SCAN_BYTES) fail('too-big');
  const sha = createHash('sha256').update(bytes).digest('hex');
  mkdirSync(dir, { recursive: true });
  const file = join(dir, sha);
  if (!existsSync(file)) {
    const part = `${file}.part`;
    writeFileSync(part, bytes);
    renameSync(part, file); // whole or not at all
  }
  const before = db.prepare('SELECT sha FROM cave_scans WHERE cave_id = ? AND path = ?').get(caveId, path) as { sha: string } | undefined;
  db.prepare(
    `INSERT INTO cave_scans (cave_id, path, sha, size, type, added_by, added_at) VALUES (?, ?, ?, ?, ?, ?, ?)
     ON CONFLICT(cave_id, path) DO UPDATE SET sha = excluded.sha, size = excluded.size, type = excluded.type, added_by = excluded.added_by, added_at = excluded.added_at`,
  ).run(caveId, path, sha, bytes.length, typeOf(path), memberId, now);
  if (before && before.sha !== sha) dropUnused(db, dir, [before.sha]);
  return { path, sha, size: bytes.length, type: typeOf(path), addedAt: now };
}

/** A page's bytes by its SHA-256, for anyone who can read a cave that has it. */
export function readScan(db: Db, dir: string, memberId: string, caveId: string, sha: string): { bytes: Buffer; type: string } {
  requireReader(db, memberId, caveId);
  if (!/^[0-9a-f]{64}$/.test(sha)) fail('not-found');
  const row = db.prepare('SELECT type FROM cave_scans WHERE cave_id = ? AND sha = ? LIMIT 1').get(caveId, sha) as { type: string } | undefined;
  if (!row) fail('not-found');
  try {
    return { bytes: readFileSync(join(dir, sha)), type: row!.type };
  } catch {
    return fail('not-found');
  }
}

/** Takes a page off the cave (writers only). */
export function removeScan(db: Db, dir: string, memberId: string, caveId: string, rawPath: unknown): void {
  requireWriter(db, memberId, caveId);
  const path = scanPath(rawPath);
  const row = db.prepare('SELECT sha FROM cave_scans WHERE cave_id = ? AND path = ?').get(caveId, path) as { sha: string } | undefined;
  if (!row) fail('not-found');
  db.prepare('DELETE FROM cave_scans WHERE cave_id = ? AND path = ?').run(caveId, path);
  dropUnused(db, dir, [row!.sha]);
}

/** Deletes every file in the scans folder that no page uses (after a cave is deleted). */
export function sweepScans(db: Db, dir: string): void {
  let names: string[];
  try {
    names = readdirSync(dir);
  } catch {
    return; // no scans kept yet
  }
  dropUnused(db, dir, names.filter((n) => /^[0-9a-f]{64}$/.test(n)));
}

/** Deletes the files of `shas` that no page uses any more. */
export function dropUnused(db: Db, dir: string, shas: string[]): void {
  for (const sha of shas) {
    if (db.prepare('SELECT 1 FROM cave_scans WHERE sha = ? LIMIT 1').get(sha)) continue;
    rmSync(join(dir, sha), { force: true });
  }
}

/** Reads a request's body as bytes, refusing more than `max`. */
async function readBytes(req: IncomingMessage, max: number): Promise<Buffer> {
  if (Number(req.headers['content-length'] ?? 0) > max) fail('too-big');
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

const STATUS: Record<string, number> = { 'not-found': 404, watcher: 403, 'bad-path': 400, 'bad-type': 400, 'bad-request': 400, 'too-big': 413 };

/**
 * `/api/caves/:cave/scans`: `GET` lists the pages, `PUT ?path=` keeps one (the body is its bytes), `DELETE ?path=`
 * takes one off. `GET /api/caves/:cave/scans/:sha` gives a page's bytes (never changing, so cached for good).
 */
export async function handleScans(db: Db, dir: string, memberId: string, req: IncomingMessage, res: ServerResponse, caveId: string, sha: string, now: number): Promise<void> {
  const url = new URL(req.url ?? '/', 'http://x');
  const method = req.method ?? 'GET';
  try {
    if (sha && method === 'GET') {
      const got = readScan(db, dir, memberId, caveId, sha);
      res.writeHead(200, { 'Content-Type': got.type, 'Content-Length': got.bytes.length, 'Cache-Control': 'private, max-age=31536000, immutable' });
      res.end(got.bytes);
      return;
    }
    if (sha) return send(res, 404, { error: 'not-found' });
    if (method === 'GET') return send(res, 200, { scans: listScans(db, memberId, caveId) });
    if (method === 'PUT') {
      requireWriter(db, memberId, caveId); // before reading a large body
      const bytes = await readBytes(req, MAX_SCAN_BYTES);
      return send(res, 200, { scan: putScan(db, dir, memberId, caveId, url.searchParams.get('path'), bytes, now) });
    }
    if (method === 'DELETE') {
      removeScan(db, dir, memberId, caveId, url.searchParams.get('path'));
      return send(res, 200, { ok: true });
    }
    return send(res, 404, { error: 'not-found' });
  } catch (e) {
    const code = e instanceof Error ? e.message : '';
    if (STATUS[code]) {
      if (code === 'too-big') req.resume(); // let the rest of the body drain so the answer arrives
      return send(res, STATUS[code], { error: code });
    }
    throw e;
  }
}
