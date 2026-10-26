// The rooms API (step 1: the Private room). The member is already known (api.ts); the device is named by the
// X-Cave-Device header and must be one of the member's own. Entries are locked text the server stores and passes on.
import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Db } from './db.ts';
import { readJson, send } from './http.ts';
import * as R from './rooms.ts';
import { broadcast, listen } from './streams.ts';

const STATUS: Record<string, number> = { 'not-found': 404, exists: 409, 'bad-device': 400, 'bad-key': 400, 'bad-id': 400, 'bad-request': 400 };
const PING_MS = 25_000;
const BATCH = 500;

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
  return v.map((w) => ({ device: text((w as R.Wrapped)?.device, 64), wrapped: text((w as R.Wrapped)?.wrapped, 4096) }));
};

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
    if (!device) throw new Error('bad-device');
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
    if (what === 'snapshot' && method === 'GET') return send(res, 200, { snapshot: R.latestSnapshot(db, memberId, cave) });
    if (what === 'stream' && method === 'GET') return stream(db, memberId, cave, Number(url.searchParams.get('after') ?? 0) || 0, req, res);
    return send(res, 404, { error: 'not-found' });
  } catch (e) {
    const code = e instanceof Error ? e.message : 'error';
    if (STATUS[code]) return send(res, STATUS[code], { error: code });
    if (e instanceof SyntaxError) return send(res, 400, { error: 'bad-request' });
    throw e;
  }
}

/** Sends every entry after `after`, then new ones as they arrive, one JSON line each, until the client goes. */
function stream(db: Db, memberId: string, cave: string, after: number, req: IncomingMessage, res: ServerResponse): void {
  R.entriesAfter(db, memberId, cave, after, 1); // checks the member may read this cave before anything is sent
  res.writeHead(200, { 'Content-Type': 'application/x-ndjson', 'Cache-Control': 'no-store', 'X-Accel-Buffering': 'no' });
  let last = after;
  for (;;) {
    const batch = R.entriesAfter(db, memberId, cave, last, BATCH);
    for (const e of batch) res.write(JSON.stringify({ t: 'entry', number: e.number, keyVersion: e.keyVersion, locked: e.locked, authorDevice: e.authorDevice }) + '\n');
    if (batch.length < BATCH) break;
    last = batch[batch.length - 1].number;
  }
  res.write(JSON.stringify({ t: 'live' }) + '\n');
  const stop = listen(cave, res);
  const ping = setInterval(() => res.write('{"t":"ping"}\n'), PING_MS);
  const end = () => {
    clearInterval(ping);
    stop();
  };
  req.on('close', end);
  res.on('close', end);
}
