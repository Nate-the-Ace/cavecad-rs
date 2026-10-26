import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { generateKeyPairSync } from 'node:crypto';
import { openDb } from '../src/db.ts';
import { handleApi, type ApiDeps } from '../src/api.ts';
import { listeners } from '../src/streams.ts';

const ADMIN = 'nathan@example.com';
const PUB = (n: number) => 'P'.repeat(30) + n;

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
  return { server, base, call };
}

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

test('snapshots, and someone else is refused', async () => {
  const { server, call } = await start();
  try {
    const d = (await call(ADMIN, 'POST', '/api/devices', { publicKey: PUB(1) })).json.device.id;
    await call(ADMIN, 'POST', '/api/caves', { id: 'cave-0002', wrapped: [{ device: d, wrapped: 'W' }] }, d);
    await call(ADMIN, 'PUT', '/api/caves/cave-0002/snapshot', { upTo: 4, version: 1, locked: 'SNAP' }, d);
    assert.deepEqual((await call(ADMIN, 'GET', '/api/caves/cave-0002/snapshot', undefined, d)).json.snapshot, { upTo: 4, keyVersion: 1, locked: 'SNAP' });
    assert.equal((await call(ADMIN, 'POST', '/api/caves/cave-0002/entries', { version: 1, locked: 'not base64!' }, d)).status, 400);
    assert.equal((await call(ADMIN, 'POST', '/api/caves/cave-0002/entries', { version: 1, locked: 'AAAA' }, 'someone-elses-device')).status, 400);
  } finally {
    server.close();
  }
});
