// The rooms API (step 1: the Private room). The member is already known (api.ts); the device is named by the
// X-Cave-Device header and must be one of the member's own. Entries are locked text the server stores and passes on.
import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Db } from './db.ts';
import { readJson, send, sendBadBody } from './http.ts';
import * as R from './rooms.ts';
import { broadcast, listen, writeOrDrop } from './streams.ts';

const STATUS: Record<string, number> = { 'not-found': 404, exists: 409, 'bad-device': 400, 'bad-key': 400, 'bad-id': 400, 'bad-request': 400 };
const PING_MS = 25_000;

/** How a stream catches up: batches of at most `rows` entries or about `bytes` of locked text. Tests shrink it and pause between batches. */
export const catchUp = { rows: 500, bytes: 4 * 1024 * 1024, afterBatch: null as null | (() => Promise<void>) };

/** A SQLite constraint error: a request that slipped past the checks above (answered 400, not 500). */
export const isConstraint = (e: unknown): boolean =>
  (e as { code?: unknown })?.code === 'ERR_SQLITE_ERROR' && ((e as { errcode?: number }).errcode ?? 0) % 256 === 19;

const text = (v: unknown, max: number): string => {
  if (typeof v !== 'string' || v.length === 0 || v.length > max || !/^[A-Za-z0-9+/=_-]+$/.test(v)) throw new Error('bad-request');
  return v;
};
const int = (v: unknown): number => {
  if (typeof v !== 'number' || !Number.isInteger(v) || v < 0) throw new Error('bad-request');
  return v;
};
const wrappedList = (v: unknown): R.Wrapped[] => {
  if (!Array.isArray(v) || v.length > 100) throw new Error('bad-request');
  const list = v.map((w) => ({ device: text((w as R.Wrapped)?.device, 64), wrapped: text((w as R.Wrapped)?.wrapped, 4096) }));
  if (new Set(list.map((w) => w.device)).size !== list.length) throw new Error('bad-request');
  return list;
};
/** A query-string number (absent is 0); anything but a whole number of at least 0 is a bad request. */
const intParam = (s: string | null): number => int(s === null ? 0 : /^\d+$/.test(s) ? Number(s) : NaN);

/** Handles `/api/devices` and `/api/caves...` for a signed-in member. */
export async function handleRooms(db: Db, memberId: string, req: IncomingMessage, res: ServerResponse, now: number): Promise<void> {
  const url = new URL(req.url ?? '/', 'http://x');
  const path = url.pathname;
  const method = req.method ?? 'GET';
  const device = String(req.headers['x-cave-device'] ?? '');
  try {
    if (path === '/api/devices' && method === 'POST') {
      return send(res, 200, { device: R.registerDevice(db, memberId, text((await readJson(req)).publicKey, 200), now) });
    }
    // Lists only the caller's own devices, so it needs no device header (a new device asks before it has one).
    if (path === '/api/devices' && method === 'GET') return send(res, 200, { devices: R.myDevices(db, memberId) });
    if (!device) throw new Error('bad-device');
    R.checkDevice(db, memberId, device, now);
    if (path === '/api/caves' && method === 'GET') return send(res, 200, { caves: R.myCaves(db, memberId, device) });
    if (path === '/api/caves' && method === 'POST') {
      const b = await readJson(req);
      R.createCave(db, memberId, text(b.id, 64), wrappedList(b.wrapped), now);
      return send(res, 200, { ok: true });
    }
    const m = /^\/api\/caves\/([A-Za-z0-9_-]{8,64})\/(missing|keys|entries|stream|snapshot)$/.exec(path);
    if (!m) return send(res, 404, { error: 'not-found' });
    const [, cave, what] = m;
    if (what === 'missing' && method === 'GET') return send(res, 200, { devices: R.devicesMissingKey(db, memberId, cave) });
    if (what === 'keys' && method === 'POST') {
      const b = await readJson(req);
      R.putWrappedKeys(db, memberId, cave, int(b.version), wrappedList(b.wrapped));
      return send(res, 200, { ok: true });
    }
    if (what === 'entries' && method === 'POST') {
      const b = await readJson(req, 2 * 1024 * 1024);
      const version = int(b.version);
      const locked = text(b.locked, 2 * 1024 * 1024);
      const number = R.appendEntry(db, memberId, device, cave, version, locked, now);
      broadcast(cave, { t: 'entry', number, keyVersion: version, locked, authorDevice: device });
      return send(res, 200, { number });
    }
    if (what === 'snapshot' && method === 'PUT') {
      const b = await readJson(req, 64 * 1024 * 1024);
      R.putSnapshot(db, memberId, cave, int(b.upTo), int(b.version), text(b.locked, 64 * 1024 * 1024), now);
      return send(res, 200, { ok: true });
    }
    if (what === 'snapshot' && method === 'GET') {
      const after = url.searchParams.get('after');
      return send(res, 200, { snapshot: R.latestSnapshot(db, memberId, cave, after === null ? undefined : intParam(after)) });
    }
    if (what === 'stream' && method === 'GET') return await stream(db, memberId, cave, intParam(url.searchParams.get('after')), res);
    return send(res, 404, { error: 'not-found' });
  } catch (e) {
    if (res.headersSent) return void res.destroy(); // a stream already under way: just end it
    const code = e instanceof Error ? e.message : 'error';
    if (STATUS[code]) return send(res, STATUS[code], { error: code });
    if (isConstraint(e)) return send(res, 400, { error: 'bad-request' });
    if (e instanceof SyntaxError) return sendBadBody(res, e);
    throw e;
  }
}

const entryLine = (e: { number: number; keyVersion: number; locked: string; authorDevice: string }) =>
  JSON.stringify({ t: 'entry', number: e.number, keyVersion: e.keyVersion, locked: e.locked, authorDevice: e.authorDevice }) + '\n';

/** Resolves when the response can take more, or has closed. */
const drained = (res: ServerResponse) =>
  new Promise<void>((resolve) => {
    const go = () => {
      res.off('drain', go);
      res.off('close', go);
      resolve();
    };
    res.on('drain', go);
    res.on('close', go);
  });

/**
 * Sends every entry after `after`, then new ones as they arrive, one JSON line each, until the client goes.
 * It listens first, so nothing posted during catch-up is missed: those lines wait in the listener's queue, and
 * going live sends only the ones catch-up did not. Catch-up reads in bounded batches and waits for the client
 * to drain, so a big history never sits whole in memory or blocks other requests.
 */
async function stream(db: Db, memberId: string, cave: string, after: number, res: ServerResponse): Promise<void> {
  R.entriesAfter(db, memberId, cave, after, 1); // checks the member may read this cave before anything is sent
  res.writeHead(200, { 'Content-Type': 'application/x-ndjson', 'Cache-Control': 'no-store', 'X-Accel-Buffering': 'no' });
  const listener = listen(cave, memberId, res);
  const ping = setInterval(() => writeOrDrop(res, '{"t":"ping"}\n'), PING_MS);
  let done = false;
  const end = () => {
    if (done) return;
    done = true;
    clearInterval(ping);
    listener.stop();
  };
  res.on('close', end);
  let last = after;
  while (!done) {
    const batch = R.entriesAfter(db, memberId, cave, last, catchUp.rows, catchUp.bytes);
    if (batch.length === 0) break;
    for (const e of batch) {
      if (done) return;
      last = e.number;
      if (!res.write(entryLine(e))) await drained(res);
    }
    if (catchUp.afterBatch) await catchUp.afterBatch();
  }
  if (!done) listener.goLive(last);
}
