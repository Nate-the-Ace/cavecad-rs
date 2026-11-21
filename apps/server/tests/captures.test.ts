// A cave's LiDAR shots on the server: resumable chunked upload, checked whole, kept once, read with ranges.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createHash, generateKeyPairSync } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { openDb } from '../src/db.ts';
import { handleApi, type ApiDeps } from '../src/api.ts';
import { createInvite, resolve } from '../src/members.ts';
import { addRoom, createCave, registerDevice, setGuest } from '../src/rooms.ts';

const T = 1_800_000_000_000;
const ADMIN = 'nathan@example.com';
const ANA = 'ana@example.com';
const BEN = 'ben@example.com';
const PUB = (n: number) => ('P'.repeat(30) + n).padEnd(43, 'x');
const sha256 = (b: Buffer) => createHash('sha256').update(b).digest('hex');

/** A small but well-formed `.cvcap`: magic, format 1, header length, header JSON (no mesh, no keyframes), then filler. */
function cvcap(cave: string, shot: string, opts: { station?: string; at?: number; filler?: number; magic?: string } = {}): Buffer {
  const wire = {
    header: {
      shot_id: shot, cave_id: cave, station_id: opts.station ?? 'st-1', station_label: 'A12', captured_at_ms: opts.at ?? T,
      device: 'iPhone 15 Pro', instrument_height_m: 1.5, start_camera: [0, 0, 0], taps: [], tracking: 'normal', arkit_version: '6', gravity: [0, -1, 0],
    },
    vertex_count: 0, normal_count: 0, triangle_count: 0, keyframes: [],
  };
  const json = Buffer.from(JSON.stringify(wire));
  const fmt = Buffer.alloc(2);
  fmt.writeUInt16LE(1);
  const len = Buffer.alloc(4);
  len.writeUInt32LE(json.length);
  const filler = Buffer.alloc(opts.filler ?? 3000);
  for (let i = 0; i < filler.length; i++) filler[i] = (i * 7 + shot.length) & 0xff;
  return Buffer.concat([Buffer.from(opts.magic ?? 'CVCAP\0', 'latin1'), fmt, len, json, filler]);
}

async function setup() {
  const db = openDb(':memory:');
  const dir = mkdtempSync(join(tmpdir(), 'cave-captures-'));
  let owner = '';
  const join2 = (email: string, name: string) => {
    if (email !== ADMIN) createInvite(db, owner, email, 'member', T);
    const r = resolve(db, { email, name, provider: 'google', subject: email }, { adminEmail: ADMIN, now: T });
    return r.status === 'member' ? r.member.id : '';
  };
  owner = join2(ADMIN, 'Nathan');
  const ana = join2(ANA, 'Ana');
  join2(BEN, 'Ben');
  const dev = registerDevice(db, owner, PUB(1), T).id;
  createCave(db, owner, 'cave-open1', [], T, 'open');
  createCave(db, owner, 'cave-open2', [], T, 'open');
  const team = addRoom(db, owner, 'cave-open1', 'Team', T);
  const deps: ApiDeps = {
    db, adminEmail: ADMIN, passKey: generateKeyPairSync('ed25519').privateKey, publicUrl: 'http://127.0.0.1', mailer: { url: '', key: '' }, capturesDir: dir,
    whoIs: async (r) => { const e = r.headers['x-test-user']; return e ? { email: String(e), name: String(e).split('@')[0], provider: null, subject: '' } : null; },
  };
  const server = createServer((req, res) => void handleApi(deps, req, res));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const call = (user: string, method: string, path: string, body?: Buffer, extra: Record<string, string> = {}) =>
    fetch(base + path, { method, headers: { 'x-test-user': user, 'Content-Type': body ? 'application/octet-stream' : 'application/json', ...extra }, body: body ? new Uint8Array(body) : undefined });
  /** Uploads a whole file in `parts` chunks; gives back the last answer. */
  const upload = async (user: string, cave: string, bytes: Buffer, parts = 1, sha = sha256(bytes)) => {
    const step = Math.ceil(bytes.length / parts);
    let res: Response | null = null;
    for (let off = 0; off < bytes.length; off += step) {
      res = await call(user, 'PUT', `/api/caves/${cave}/captures/${sha}?offset=${off}&total=${bytes.length}`, bytes.subarray(off, off + step));
      if (res.status !== 200) return res;
      if (off + step < bytes.length) await res.json();
    }
    return res!;
  };
  return { db, dir, owner, ana, team, dev, call, upload, close: () => server.close(), setGuest: (seat: 'editor' | 'watcher') => setGuest(db, owner, 'cave-open1', team, ana, seat, T) };
}

test('the list starts empty', async () => {
  const s = await setup();
  try {
    const res = await s.call(ADMIN, 'GET', '/api/caves/cave-open1/captures');
    assert.equal(res.status, 200);
    assert.deepEqual(await res.json(), { captures: [] });
  } finally {
    s.close();
  }
});

test('chunked upload in three parts, resuming after a wrong offset; listed and downloadable with a range', async () => {
  const s = await setup();
  try {
    const file = cvcap('cave-open1', 'shot-1');
    const sha = sha256(file);
    const third = Math.ceil(file.length / 3);
    const put = (off: number, part: Buffer) => s.call(ADMIN, 'PUT', `/api/caves/cave-open1/captures/${sha}?offset=${off}&total=${file.length}`, part);
    const one = await put(0, file.subarray(0, third));
    assert.equal(one.status, 200);
    assert.deepEqual(await one.json(), { have: third });
    // the client lost track and sends the third part: told where to resume
    const wrong = await put(2 * third, file.subarray(2 * third));
    assert.equal(wrong.status, 409);
    assert.deepEqual(await wrong.json(), { error: 'offset', have: third });
    const asked = await s.call(ADMIN, 'GET', `/api/caves/cave-open1/captures/${sha}/upload`);
    assert.deepEqual(await asked.json(), { have: third });
    const head = await s.call(ADMIN, 'HEAD', `/api/caves/cave-open1/captures/${sha}/upload`);
    assert.equal(head.status, 200);
    assert.equal(head.headers.get('upload-offset'), String(third));
    assert.equal((await put(third, file.subarray(third, 2 * third))).status, 200);
    const done = await put(2 * third, file.subarray(2 * third));
    assert.equal(done.status, 200);
    const kept = (await done.json()).capture;
    assert.equal(kept.shotId, 'shot-1');
    assert.equal(kept.sha, sha);
    assert.equal(kept.size, file.length);
    assert.equal(kept.stationId, 'st-1');
    assert.equal(kept.stationLabel, 'A12');
    assert.equal(kept.capturedAt, T);
    assert.equal(kept.device, 'iPhone 15 Pro');
    assert.ok(existsSync(join(s.dir, sha)));
    assert.deepEqual(readdirSync(join(s.dir, 'uploads')), []);
    const list = (await (await s.call(ADMIN, 'GET', '/api/caves/cave-open1/captures')).json()).captures;
    assert.deepEqual(list.map((c: { shotId: string }) => c.shotId), ['shot-1']);
    assert.equal(list[0].addedBy, s.owner);
    // whole download
    const whole = await s.call(ADMIN, 'GET', `/api/caves/cave-open1/captures/${sha}`);
    assert.equal(whole.status, 200);
    assert.equal(whole.headers.get('cache-control'), 'private, max-age=31536000, immutable');
    assert.deepEqual(Buffer.from(await whole.arrayBuffer()), file);
    // ranges, for a phone resuming its download
    const part = await s.call(ADMIN, 'GET', `/api/caves/cave-open1/captures/${sha}`, undefined, { Range: 'bytes=10-99' });
    assert.equal(part.status, 206);
    assert.equal(part.headers.get('content-range'), `bytes 10-99/${file.length}`);
    assert.deepEqual(Buffer.from(await part.arrayBuffer()), file.subarray(10, 100));
    const tail = await s.call(ADMIN, 'GET', `/api/caves/cave-open1/captures/${sha}`, undefined, { Range: `bytes=${file.length - 5}-` });
    assert.equal(tail.status, 206);
    assert.deepEqual(Buffer.from(await tail.arrayBuffer()), file.subarray(file.length - 5));
    const past = await s.call(ADMIN, 'GET', `/api/caves/cave-open1/captures/${sha}`, undefined, { Range: `bytes=${file.length}-` });
    assert.equal(past.status, 416);
    await past.arrayBuffer();
    // never through another cave
    assert.equal((await s.call(ADMIN, 'GET', `/api/caves/cave-open2/captures/${sha}`)).status, 404);
    // sending the same bytes again finishes at once (a lost final answer, or the same shot twice)
    const again = await s.call(ADMIN, 'PUT', `/api/caves/cave-open1/captures/${sha}?offset=0&total=${file.length}`, file.subarray(0, 10));
    assert.equal(again.status, 200);
    assert.equal((await again.json()).capture.shotId, 'shot-1');
  } finally {
    s.close();
  }
});

test('refused files: wrong SHA, not a capture, another cave in the header; the part is thrown away', async () => {
  const s = await setup();
  try {
    const file = cvcap('cave-open1', 'shot-1');
    const bad = await s.upload(ADMIN, 'cave-open1', file, 2, 'a'.repeat(64));
    assert.equal(bad.status, 422);
    assert.deepEqual(await bad.json(), { error: 'bad-sha' });
    const notCap = await s.upload(ADMIN, 'cave-open1', cvcap('cave-open1', 'shot-1', { magic: 'NOTCAP' }));
    assert.equal(notCap.status, 422);
    assert.deepEqual(await notCap.json(), { error: 'bad-capture' });
    const garbage = await s.upload(ADMIN, 'cave-open1', Buffer.concat([Buffer.from('CVCAP\0', 'latin1'), Buffer.from([1, 0, 50, 0, 0, 0]), Buffer.from('{not json')]));
    assert.equal(garbage.status, 422);
    const other = await s.upload(ADMIN, 'cave-open1', cvcap('cave-open2', 'shot-1'));
    assert.equal(other.status, 422);
    assert.deepEqual(await other.json(), { error: 'wrong-cave' });
    assert.deepEqual(readdirSync(join(s.dir, 'uploads')), []);
    assert.deepEqual(readdirSync(s.dir), ['uploads']);
    assert.deepEqual((await (await s.call(ADMIN, 'GET', '/api/caves/cave-open1/captures')).json()).captures, []);
  } finally {
    s.close();
  }
});

test('the same shot again replaces its row and drops the old file; newest first', async () => {
  const s = await setup();
  try {
    const first = cvcap('cave-open1', 'shot-1', { station: '' });
    assert.equal((await s.upload(ADMIN, 'cave-open1', first, 2)).status, 200);
    const other = cvcap('cave-open1', 'shot-2', { at: T + 5000 });
    assert.equal((await s.upload(ADMIN, 'cave-open1', other)).status, 200);
    const placed = cvcap('cave-open1', 'shot-1', { station: 'st-9' });
    assert.equal((await s.upload(ADMIN, 'cave-open1', placed, 3)).status, 200);
    assert.ok(!existsSync(join(s.dir, sha256(first))), 'the old file went');
    const list = (await (await s.call(ADMIN, 'GET', '/api/caves/cave-open1/captures')).json()).captures;
    assert.deepEqual(list.map((c: { shotId: string; stationId: string }) => [c.shotId, c.stationId]), [['shot-2', 'st-1'], ['shot-1', 'st-9']]);
  } finally {
    s.close();
  }
});

test('watchers read but cannot upload or delete; strangers see nothing', async () => {
  const s = await setup();
  try {
    const file = cvcap('cave-open1', 'shot-1');
    assert.equal((await s.upload(ADMIN, 'cave-open1', file)).status, 200);
    assert.equal((await s.call(ANA, 'GET', '/api/caves/cave-open1/captures')).status, 404);
    assert.equal((await s.upload(ANA, 'cave-open1', cvcap('cave-open1', 'shot-2'))).status, 404);
    assert.equal((await s.call(BEN, 'GET', `/api/caves/cave-open1/captures/${sha256(file)}`)).status, 404);
    s.setGuest('watcher');
    assert.equal((await s.call(ANA, 'GET', '/api/caves/cave-open1/captures')).status, 200);
    assert.equal((await s.call(ANA, 'GET', `/api/caves/cave-open1/captures/${sha256(file)}`)).status, 200);
    const up = await s.upload(ANA, 'cave-open1', cvcap('cave-open1', 'shot-2'));
    assert.equal(up.status, 403);
    assert.equal((await s.call(ANA, 'GET', `/api/caves/cave-open1/captures/${sha256(file)}/upload`)).status, 403);
    assert.equal((await s.call(ANA, 'DELETE', '/api/caves/cave-open1/captures/shot-1')).status, 403);
    s.setGuest('editor');
    assert.equal((await s.upload(ANA, 'cave-open1', cvcap('cave-open1', 'shot-2'))).status, 200);
  } finally {
    s.close();
  }
});

test('delete a shot; a file another cave uses stays; deleting a cave sweeps its files and parts', async () => {
  const s = await setup();
  try {
    const a = cvcap('cave-open1', 'shot-1');
    const b = cvcap('cave-open2', 'shot-1');
    assert.equal((await s.upload(ADMIN, 'cave-open1', a)).status, 200);
    assert.equal((await s.upload(ADMIN, 'cave-open2', b)).status, 200);
    assert.equal((await s.call(ADMIN, 'DELETE', '/api/caves/cave-open1/captures/nope')).status, 404);
    assert.equal((await s.call(ADMIN, 'DELETE', '/api/caves/cave-open1/captures/shot-1')).status, 200);
    assert.ok(!existsSync(join(s.dir, sha256(a))));
    assert.deepEqual((await (await s.call(ADMIN, 'GET', '/api/caves/cave-open1/captures')).json()).captures, []);
    // a half-sent upload of cave-open2 and its kept file both go with the cave
    const c = cvcap('cave-open2', 'shot-2');
    await s.call(ADMIN, 'PUT', `/api/caves/cave-open2/captures/${sha256(c)}?offset=0&total=${c.length}`, c.subarray(0, 100));
    assert.equal(readdirSync(join(s.dir, 'uploads')).length, 1);
    const del = await s.call(ADMIN, 'DELETE', '/api/caves/cave-open2', undefined, { 'X-Cave-Device': s.dev });
    assert.equal(del.status, 200);
    assert.ok(!existsSync(join(s.dir, sha256(b))));
    assert.deepEqual(readdirSync(join(s.dir, 'uploads')), []);
    assert.equal((s.db.prepare('SELECT COUNT(*) AS n FROM cave_captures').get() as { n: number }).n, 0);
  } finally {
    s.close();
  }
});

test('limits: total over 512 MiB, a chunk over 8 MiB, a bad offset; stale parts are swept', async () => {
  const s = await setup();
  try {
    const sha = 'b'.repeat(64);
    const huge = await s.call(ADMIN, 'PUT', `/api/caves/cave-open1/captures/${sha}?offset=0&total=${512 * 1024 * 1024 + 1}`, Buffer.alloc(10));
    assert.equal(huge.status, 413);
    const chunk = await s.call(ADMIN, 'PUT', `/api/caves/cave-open1/captures/${sha}?offset=0&total=${20 * 1024 * 1024}`, Buffer.alloc(8 * 1024 * 1024 + 1));
    assert.equal(chunk.status, 413);
    const over = await s.call(ADMIN, 'PUT', `/api/caves/cave-open1/captures/${sha}?offset=0&total=5`, Buffer.alloc(10));
    assert.equal(over.status, 400);
    assert.equal((await s.call(ADMIN, 'PUT', `/api/caves/cave-open1/captures/${sha}?offset=-1&total=5`, Buffer.alloc(1))).status, 400);
    // a form-like type is still refused
    assert.equal((await s.call(ADMIN, 'PUT', `/api/caves/cave-open1/captures/${sha}?offset=0&total=5`, Buffer.alloc(1), { 'Content-Type': 'text/plain' })).status, 415);
    // a part left two days ago is swept when an upload starts
    mkdirSync(join(s.dir, 'uploads'), { recursive: true });
    const stale = join(s.dir, 'uploads', `cave-open1-${'c'.repeat(64)}.part`);
    writeFileSync(stale, 'x');
    const old = (Date.now() - 49 * 3600 * 1000) / 1000;
    utimesSync(stale, old, old);
    assert.equal((await s.upload(ADMIN, 'cave-open1', cvcap('cave-open1', 'shot-1'))).status, 200);
    assert.ok(!existsSync(stale));
  } finally {
    s.close();
  }
});
