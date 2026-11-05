// A cave's scanned pages on the server: who may add, look and remove; files kept once and dropped when unused.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { generateKeyPairSync } from 'node:crypto';
import { existsSync, mkdtempSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { openDb } from '../src/db.ts';
import { handleApi, type ApiDeps } from '../src/api.ts';
import { createInvite, resolve } from '../src/members.ts';
import { addRoom, createCave, deleteCave, registerDevice, setGuest } from '../src/rooms.ts';
import { listScans, putScan, readScan, removeScan, scanPath, sweepScans } from '../src/scans.ts';

const T = 1_800_000_000_000;
const ADMIN = 'nathan@example.com';
const PUB = (n: number) => ('P'.repeat(30) + n).padEnd(43, 'x');
const PNG = Buffer.from('89504e470d0a1a0a0000000d49484452', 'hex');

function people() {
  const db = openDb(':memory:');
  const dir = mkdtempSync(join(tmpdir(), 'cave-scans-'));
  let owner = '';
  const join2 = (email: string, name: string) => {
    if (email !== ADMIN) createInvite(db, owner, email, 'member', T);
    const r = resolve(db, { email, name, provider: 'google', subject: email }, { adminEmail: ADMIN, now: T });
    return r.status === 'member' ? r.member.id : '';
  };
  owner = join2(ADMIN, 'Nathan');
  const ana = join2('ana@example.com', 'Ana');
  const ben = join2('ben@example.com', 'Ben');
  registerDevice(db, owner, PUB(1), T);
  createCave(db, owner, 'cave-open1', [], T, 'open');
  createCave(db, owner, 'cave-open2', [], T, 'open');
  const team = addRoom(db, owner, 'cave-open1', 'Team', T);
  return { db, dir, owner, ana, ben, team };
}

test('page paths: relative, no climbing out, a scan kind', () => {
  assert.equal(scanPath('Trip 1/page 1.PNG'), 'Trip 1/page 1.PNG');
  for (const bad of ['', '/abs.png', '../up.png', 'a/../b.png', 'a//b.png', 'a\\b.png', 'notes.txt', 'x'.repeat(301) + '.png', 3]) assert.throws(() => scanPath(bad));
});

test('owners and editors add, everyone in the cave looks, watchers and strangers cannot add', () => {
  const { db, dir, owner, ana, ben, team } = people();
  const kept = putScan(db, dir, owner, 'cave-open1', 'trip1/p1.png', PNG, T);
  assert.equal(kept.type, 'image/png');
  assert.deepEqual(listScans(db, owner, 'cave-open1').map((s) => s.path), ['trip1/p1.png']);
  assert.throws(() => listScans(db, ana, 'cave-open1'), /not-found/);
  setGuest(db, owner, 'cave-open1', team, ana, 'watcher', T);
  assert.equal(readScan(db, dir, ana, 'cave-open1', kept.sha).bytes.length, PNG.length);
  assert.throws(() => putScan(db, dir, ana, 'cave-open1', 'p2.png', PNG, T), /watcher/);
  assert.throws(() => removeScan(db, dir, ana, 'cave-open1', 'trip1/p1.png'), /watcher/);
  setGuest(db, owner, 'cave-open1', team, ana, 'editor', T);
  putScan(db, dir, ana, 'cave-open1', 'p2.png', PNG, T);
  assert.throws(() => putScan(db, dir, ben, 'cave-open1', 'p3.png', PNG, T), /not-found/);
  // a page is read through a cave that has it, never another
  assert.throws(() => readScan(db, dir, owner, 'cave-open2', kept.sha), /not-found/);
});

test('the same picture is one file; it goes when no page uses it, and with a deleted cave', () => {
  const { db, dir, owner } = people();
  const a = putScan(db, dir, owner, 'cave-open1', 'a.png', PNG, T);
  putScan(db, dir, owner, 'cave-open2', 'b.png', PNG, T);
  assert.deepEqual(readdirSync(dir), [a.sha]);
  removeScan(db, dir, owner, 'cave-open1', 'a.png');
  assert.ok(existsSync(join(dir, a.sha)), 'still used by the other cave');
  deleteCave(db, owner, 'cave-open2', T);
  sweepScans(db, dir);
  assert.ok(!existsSync(join(dir, a.sha)), 'gone with the last cave that used it');
  // replacing a page drops the old picture
  const one = putScan(db, dir, owner, 'cave-open1', 'p.png', PNG, T);
  putScan(db, dir, owner, 'cave-open1', 'p.png', Buffer.concat([PNG, Buffer.from([1])]), T);
  assert.ok(!existsSync(join(dir, one.sha)));
});

test('over HTTP: upload bytes, list, fetch, remove; JSON-only guard still holds elsewhere; too big is refused', async () => {
  const db = openDb(':memory:');
  const dir = mkdtempSync(join(tmpdir(), 'cave-scans-'));
  const deps: ApiDeps = {
    db, adminEmail: ADMIN, passKey: generateKeyPairSync('ed25519').privateKey, publicUrl: 'http://127.0.0.1', mailer: { url: '', key: '' }, scansDir: dir,
    whoIs: async (r) => { const e = r.headers['x-test-user']; return e ? { email: String(e), name: String(e).split('@')[0], provider: null, subject: '' } : null; },
  };
  const server = createServer((req, res) => void handleApi(deps, req, res));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const h = (type = 'application/json', device = '') => ({ 'x-test-user': ADMIN, 'Content-Type': type, ...(device ? { 'X-Cave-Device': device } : {}) });
  try {
    const dev = (await (await fetch(base + '/api/devices', { method: 'POST', headers: h(), body: JSON.stringify({ publicKey: PUB(1) }) })).json()).device.id;
    await fetch(base + '/api/caves', { method: 'POST', headers: h('application/json', dev), body: JSON.stringify({ id: 'cave-open1', storage: 'open' }) });
    const up = await fetch(base + '/api/caves/cave-open1/scans?path=' + encodeURIComponent('Trip 1/p 1.png'), { method: 'PUT', headers: h('application/octet-stream'), body: PNG });
    assert.equal(up.status, 200);
    const sha = (await up.json()).scan.sha;
    const list = await (await fetch(base + '/api/caves/cave-open1/scans', { headers: h() })).json();
    assert.deepEqual(list.scans.map((s: { path: string }) => s.path), ['Trip 1/p 1.png']);
    const got = await fetch(`${base}/api/caves/cave-open1/scans/${sha}`, { headers: h() });
    assert.equal(got.headers.get('content-type'), 'image/png');
    assert.deepEqual(Buffer.from(await got.arrayBuffer()), PNG);
    // bytes go only to a cave's scans; a form-like type is refused
    assert.equal((await fetch(base + '/api/caves/cave-open1/scans?path=x.png', { method: 'PUT', headers: h('text/plain'), body: 'x' })).status, 415);
    assert.equal((await fetch(base + '/api/caves', { method: 'POST', headers: h('application/octet-stream', dev), body: 'x' })).status, 415);
    const big = await fetch(base + '/api/caves/cave-open1/scans?path=big.png', { method: 'PUT', headers: h('application/octet-stream'), body: Buffer.alloc(25 * 1024 * 1024 + 1) });
    assert.equal(big.status, 413);
    const del = await fetch(base + '/api/caves/cave-open1/scans?path=' + encodeURIComponent('Trip 1/p 1.png'), { method: 'DELETE', headers: h() });
    assert.equal(del.status, 200);
    assert.deepEqual(readdirSync(dir), []);
  } finally {
    server.close();
  }
});
