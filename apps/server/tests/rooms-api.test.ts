import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { generateKeyPairSync } from 'node:crypto';
import { openDb } from '../src/db.ts';
import { handleApi, type ApiDeps } from '../src/api.ts';
import { listeners } from '../src/streams.ts';
import { catchUp, isConstraint } from '../src/rooms-api.ts';
import { createInvite } from '../src/members.ts';

const ADMIN = 'nathan@example.com';
const PUB = (n: number) => ('P'.repeat(30) + n).padEnd(43, 'x');

async function start() {
  const db = openDb(':memory:');
  const deps: ApiDeps = {
    db, adminEmail: ADMIN, passKey: generateKeyPairSync('ed25519').privateKey, publicUrl: 'http://127.0.0.1', mailer: { url: '', key: '' },
    whoIs: async (r) => { const e = r.headers['x-test-user']; return e ? { email: String(e), name: 'n', provider: null, subject: '' } : null; },
  };
  const server = createServer((req, res) => void handleApi(deps, req, res));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const call = async (user: string, method: string, path: string, body?: unknown, device = '') => {
    const res = await fetch(base + path, { method, headers: { 'x-test-user': user, 'Content-Type': 'application/json', ...(device ? { 'X-Cave-Device': device } : {}) }, body: body === undefined ? undefined : JSON.stringify(body) });
    return { status: res.status, json: await res.json() };
  };
  /** Makes Rick a member and gives back one of his devices. */
  const rickDevice = async () => {
    const admin = (await call(ADMIN, 'GET', '/api/me')).json.member.id;
    createInvite(db, admin, RICK, 'member', Date.now());
    return (await call(RICK, 'POST', '/api/devices', { publicKey: PUB(9) })).json.device.id as string;
  };
  return { server, base, call, db, rickDevice };
}

const RICK = 'rick@example.com';

async function readLines(res: Response, count: number): Promise<Record<string, unknown>[]> {
  const out: Record<string, unknown>[] = [];
  const reader = res.body!.getReader();
  let buf = '';
  while (out.length < count) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += new TextDecoder().decode(value);
    let i;
    while ((i = buf.indexOf('\n')) >= 0 && out.length < count) {
      out.push(JSON.parse(buf.slice(0, i)));
      buf = buf.slice(i + 1);
    }
  }
  await reader.cancel();
  return out;
}

test('a device makes a cave, posts entries, and a stream catches up then goes live', async () => {
  const { server, base, call } = await start();
  try {
    const laptop = (await call(ADMIN, 'POST', '/api/devices', { publicKey: PUB(1) })).json.device.id;
    const tablet = (await call(ADMIN, 'POST', '/api/devices', { publicKey: PUB(2) })).json.device.id;
    assert.equal((await call(ADMIN, 'GET', '/api/caves')).json.error, 'bad-device');
    assert.equal((await call(ADMIN, 'POST', '/api/caves', { id: 'cave-0001', wrapped: [{ device: laptop, wrapped: 'W1' }] }, laptop)).status, 200);
    assert.equal((await call(ADMIN, 'POST', '/api/caves', { id: 'cave-0001', wrapped: [] }, laptop)).status, 409);
    assert.deepEqual((await call(ADMIN, 'GET', '/api/caves/cave-0001/missing', undefined, laptop)).json.devices.map((d: { id: string }) => d.id), [tablet]);
    await call(ADMIN, 'POST', '/api/caves/cave-0001/keys', { version: 1, wrapped: [{ device: tablet, wrapped: 'W2' }] }, laptop);
    assert.equal((await call(ADMIN, 'GET', '/api/caves', undefined, tablet)).json.caves[0].wrapped, 'W2');

    assert.equal((await call(ADMIN, 'POST', '/api/caves/cave-0001/entries', { version: 1, locked: 'AAAA' }, laptop)).json.number, 1);
    const res = await fetch(base + '/api/caves/cave-0001/stream?after=0', { headers: { 'x-test-user': ADMIN, 'X-Cave-Device': tablet } });
    assert.equal(res.headers.get('content-type'), 'application/x-ndjson');
    const linesP = readLines(res, 3);
    await new Promise((r) => setTimeout(r, 50));
    assert.equal(listeners('cave-0001'), 1);
    await call(ADMIN, 'POST', '/api/caves/cave-0001/entries', { version: 1, locked: 'BBBB' }, laptop);
    const lines = await linesP;
    assert.deepEqual(lines.map((l) => [l.t, l.number, l.locked]), [['entry', 1, 'AAAA'], ['live', undefined, undefined], ['entry', 2, 'BBBB']]);
    assert.equal(lines[2].authorDevice, laptop);
  } finally {
    server.close();
  }
});

test('GET /api/devices lists the member\'s own devices with their public keys, and no one else\'s', async () => {
  const { server, call, rickDevice } = await start();
  try {
    const laptop = (await call(ADMIN, 'POST', '/api/devices', { publicKey: PUB(1) })).json.device.id;
    const tablet = (await call(ADMIN, 'POST', '/api/devices', { publicKey: PUB(2) })).json.device.id;
    await rickDevice();
    const r = await call(ADMIN, 'GET', '/api/devices');
    assert.equal(r.status, 200);
    assert.deepEqual(r.json.devices, [{ id: laptop, publicKey: PUB(1) }, { id: tablet, publicKey: PUB(2) }]);
  } finally {
    server.close();
  }
});

test('snapshots, and someone else is refused', async () => {
  const { server, call } = await start();
  try {
    const d = (await call(ADMIN, 'POST', '/api/devices', { publicKey: PUB(1) })).json.device.id;
    await call(ADMIN, 'POST', '/api/caves', { id: 'cave-0002', wrapped: [{ device: d, wrapped: 'W' }] }, d);
    for (let i = 0; i < 4; i++) await call(ADMIN, 'POST', '/api/caves/cave-0002/entries', { version: 1, locked: 'AAAA' }, d);
    await call(ADMIN, 'PUT', '/api/caves/cave-0002/snapshot', { upTo: 4, version: 1, locked: 'SNAP' }, d);
    assert.deepEqual((await call(ADMIN, 'GET', '/api/caves/cave-0002/snapshot', undefined, d)).json.snapshot, { upTo: 4, keyVersion: 1, locked: 'SNAP' });
    // A reconnecting client passes its last number: nothing comes back unless the snapshot covers more.
    assert.equal((await call(ADMIN, 'GET', '/api/caves/cave-0002/snapshot?after=4', undefined, d)).json.snapshot, null);
    assert.equal((await call(ADMIN, 'GET', '/api/caves/cave-0002/snapshot?after=3', undefined, d)).json.snapshot.upTo, 4);
    assert.equal((await call(ADMIN, 'GET', '/api/caves/cave-0002/snapshot?after=x', undefined, d)).status, 400);
    assert.equal((await call(ADMIN, 'POST', '/api/devices', { publicKey: 'Q'.repeat(42) })).status, 400);
    assert.equal((await call(ADMIN, 'POST', '/api/caves/cave-0002/entries', { version: 1, locked: 'not base64!' }, d)).status, 400);
    assert.equal((await call(ADMIN, 'POST', '/api/caves/cave-0002/entries', { version: 1, locked: 'AAAA' }, 'someone-elses-device')).status, 400);
  } finally {
    server.close();
  }
});

test('the device header is checked on every route', async () => {
  const { server, base, call, rickDevice } = await start();
  try {
    const d = (await call(ADMIN, 'POST', '/api/devices', { publicKey: PUB(1) })).json.device.id;
    await call(ADMIN, 'POST', '/api/caves', { id: 'cave-0003', wrapped: [{ device: d, wrapped: 'W' }] }, d);
    const foreign = await rickDevice();
    for (const bad of ['made-up-device', foreign]) {
      for (const path of ['/api/caves', '/api/caves/cave-0003/missing', '/api/caves/cave-0003/snapshot']) {
        const r = await call(ADMIN, 'GET', path, undefined, bad);
        assert.deepEqual([r.status, r.json.error], [400, 'bad-device'], `${path} with ${bad}`);
      }
      const s = await fetch(base + '/api/caves/cave-0003/stream', { headers: { 'x-test-user': ADMIN, 'X-Cave-Device': bad } });
      assert.deepEqual([s.status, (await s.json()).error], [400, 'bad-device'], `stream with ${bad}`);
    }
    assert.equal(listeners('cave-0003'), 0);
  } finally {
    server.close();
  }
});

test('client mistakes are 400s', async () => {
  const { server, base, call } = await start();
  try {
    const d = (await call(ADMIN, 'POST', '/api/devices', { publicKey: PUB(1) })).json.device.id;
    const t = (await call(ADMIN, 'POST', '/api/devices', { publicKey: PUB(2) })).json.device.id;
    const twice = [{ device: d, wrapped: 'W' }, { device: d, wrapped: 'V' }];
    assert.deepEqual((await call(ADMIN, 'POST', '/api/caves', { id: 'cave-0004', wrapped: twice }, d)).json, { error: 'bad-request' });
    await call(ADMIN, 'POST', '/api/caves', { id: 'cave-0004', wrapped: [{ device: d, wrapped: 'W' }] }, d);
    assert.equal((await call(ADMIN, 'POST', '/api/caves/cave-0004/keys', { version: 1, wrapped: [{ device: t, wrapped: 'A' }, { device: t, wrapped: 'B' }] }, d)).status, 400);
    assert.equal((await call(ADMIN, 'POST', '/api/caves/cave-0004/keys', { version: 2, wrapped: [{ device: t, wrapped: 'A' }] }, d)).status, 400);
    assert.equal((await call(ADMIN, 'POST', '/api/caves/cave-0004/entries', { version: 2, locked: 'AAAA' }, d)).status, 400);
    assert.equal((await call(ADMIN, 'PUT', '/api/caves/cave-0004/snapshot', { upTo: 1, version: 1, locked: 'SNAP' }, d)).status, 400);
    for (const after of ['abc', '-1', '1.5', '']) {
      const s = await fetch(base + `/api/caves/cave-0004/stream?after=${after}`, { headers: { 'x-test-user': ADMIN, 'X-Cave-Device': d } });
      assert.equal(s.status, 400, `after=${after}`);
      await s.body?.cancel();
    }
    assert.equal(listeners('cave-0004'), 0);
  } finally {
    server.close();
  }
});

test('a database constraint error counts as a client mistake', () => {
  const db = openDb(':memory:');
  db.exec('CREATE TABLE t (a PRIMARY KEY)');
  db.exec('INSERT INTO t VALUES (1)');
  let err: unknown;
  try {
    db.exec('INSERT INTO t VALUES (1)');
  } catch (e) {
    err = e;
  }
  assert.equal(isConstraint(err), true);
  assert.equal(isConstraint(new Error('boom')), false);
});

test('an entry posted while the stream is still catching up arrives exactly once, in order', async () => {
  const { server, base, call } = await start();
  const saved = { ...catchUp };
  try {
    const d = (await call(ADMIN, 'POST', '/api/devices', { publicKey: PUB(1) })).json.device.id;
    await call(ADMIN, 'POST', '/api/caves', { id: 'cave-0005', wrapped: [{ device: d, wrapped: 'W' }] }, d);
    for (let i = 0; i < 3; i++) await call(ADMIN, 'POST', '/api/caves/cave-0005/entries', { version: 1, locked: 'AAAA' }, d);
    let paused = 0;
    catchUp.rows = 1;
    catchUp.afterBatch = async () => {
      // the first pause: entry 4 lands mid catch-up, after this stream started listening
      if (paused++ === 0) assert.equal((await call(ADMIN, 'POST', '/api/caves/cave-0005/entries', { version: 1, locked: 'BBBB' }, d)).json.number, 4);
    };
    const res = await fetch(base + '/api/caves/cave-0005/stream?after=0', { headers: { 'x-test-user': ADMIN, 'X-Cave-Device': d } });
    const linesP = readLines(res, 6);
    while (listeners('cave-0005') === 0 || paused < 4) await new Promise((r) => setTimeout(r, 10));
    await new Promise((r) => setTimeout(r, 30));
    await call(ADMIN, 'POST', '/api/caves/cave-0005/entries', { version: 1, locked: 'CCCC' }, d);
    const lines = (await linesP).filter((l) => l.t !== 'ping');
    assert.deepEqual(lines.map((l) => (l.t === 'entry' ? l.number : l.t)), [1, 2, 3, 4, 'live', 5]);
  } finally {
    Object.assign(catchUp, saved);
    server.close();
  }
});

test('closing a stream stops listening; another stream on the cave carries on', async () => {
  const { server, base, call } = await start();
  try {
    const d = (await call(ADMIN, 'POST', '/api/devices', { publicKey: PUB(1) })).json.device.id;
    await call(ADMIN, 'POST', '/api/caves', { id: 'cave-0006', wrapped: [{ device: d, wrapped: 'W' }] }, d);
    const open = () => fetch(base + '/api/caves/cave-0006/stream', { headers: { 'x-test-user': ADMIN, 'X-Cave-Device': d } });
    const a = await open();
    const b = await open();
    const bLines = readLines(b, 2);
    while (listeners('cave-0006') < 2) await new Promise((r) => setTimeout(r, 10));
    await a.body!.cancel();
    while (listeners('cave-0006') > 1) await new Promise((r) => setTimeout(r, 10));
    await call(ADMIN, 'POST', '/api/caves/cave-0006/entries', { version: 1, locked: 'AAAA' }, d);
    assert.deepEqual((await bLines).map((l) => l.number ?? l.t), ['live', 1]);
    while (listeners('cave-0006') > 0) await new Promise((r) => setTimeout(r, 10));
  } finally {
    server.close();
  }
});
