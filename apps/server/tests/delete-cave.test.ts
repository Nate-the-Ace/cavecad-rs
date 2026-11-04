// Deleting a cave (its owner, for good, everywhere) and leaving one (a guest, for themself).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { generateKeyPairSync } from 'node:crypto';
import { openDb } from '../src/db.ts';
import { handleApi, type ApiDeps } from '../src/api.ts';
import { createInvite, resolve } from '../src/members.ts';
import { addRoom, appendEntry, createCave, deleteCave, deletedCaves, entriesAfter, goneCaves, latestSnapshot, leaveCave, myCaves, putLabel, putSnapshot, registerDevice, setGuest } from '../src/rooms.ts';

const T = 1_800_000_000_000;
const ADMIN = 'nathan@example.com';
const PUB = (n: number) => ('P'.repeat(30) + n).padEnd(43, 'x');

function people() {
  const db = openDb(':memory:');
  let owner = '';
  const join = (email: string, name: string) => {
    if (email !== ADMIN) createInvite(db, owner, email, 'member', T);
    const r = resolve(db, { email, name, provider: 'google', subject: email }, { adminEmail: ADMIN, now: T });
    return r.status === 'member' ? r.member.id : '';
  };
  owner = join(ADMIN, 'Nathan');
  const ana = join('ana@example.com', 'Ana');
  const ben = join('ben@example.com', 'Ben');
  const dev = registerDevice(db, owner, PUB(1), T).id;
  const anaDev = registerDevice(db, ana, PUB(2), T).id;
  createCave(db, owner, 'cave-open1', [], T, 'open');
  const team = addRoom(db, owner, 'cave-open1', 'Team', T);
  setGuest(db, owner, 'cave-open1', team, ana, 'editor', T);
  appendEntry(db, owner, dev, 'cave-open1', 0, '{"c":"a","ops":"[]"}', T);
  putSnapshot(db, owner, 'cave-open1', 1, 0, '[]', T);
  putLabel(db, owner, 'cave-open1', 0, 'Test cave', T);
  return { db, owner, ana, ben, dev, anaDev };
}

test('only an owner deletes; everything goes but a tombstone, and nobody can make the cave again', () => {
  const { db, owner, ana, ben, dev, anaDev } = people();
  assert.throws(() => deleteCave(db, ana, 'cave-open1', T), /not-found/); // a guest cannot
  assert.throws(() => deleteCave(db, ben, 'cave-open1', T), /not-found/); // nor a stranger
  const told = deleteCave(db, owner, 'cave-open1', T + 1);
  assert.deepEqual(new Set(told), new Set([owner, ana]));
  assert.deepEqual(myCaves(db, owner, dev), []);
  assert.deepEqual(myCaves(db, ana, anaDev), []);
  assert.deepEqual(deletedCaves(db, owner), ['cave-open1']);
  assert.deepEqual(deletedCaves(db, ana), ['cave-open1']);
  assert.deepEqual(deletedCaves(db, ben), []);
  assert.deepEqual(goneCaves(db, ana), ['cave-open1']);
  assert.throws(() => entriesAfter(db, owner, 'cave-open1', 0, 10), /not-found/);
  assert.throws(() => latestSnapshot(db, owner, 'cave-open1'), /not-found/);
  assert.throws(() => appendEntry(db, owner, dev, 'cave-open1', 0, '{}', T), /not-found/);
  for (const t of ['history', 'snapshots', 'cave_labels', 'wrapped_keys', 'cave_owners', 'rooms']) {
    assert.equal((db.prepare(`SELECT COUNT(*) AS n FROM ${t}`).get() as { n: number }).n, 0, t);
  }
  // an old copy synced from another device cannot bring it back
  assert.throws(() => createCave(db, owner, 'cave-open1', [], T, 'open'), /exists/);
  assert.throws(() => deleteCave(db, owner, 'cave-open1', T), /not-found/);
});

test('a guest leaves a shared cave; an owner cannot leave their own', () => {
  const { db, owner, ana, ben, anaDev } = people();
  assert.throws(() => leaveCave(db, owner, 'cave-open1', T), /is-owner/);
  assert.throws(() => leaveCave(db, ben, 'cave-open1', T), /not-found/);
  leaveCave(db, ana, 'cave-open1', T);
  assert.deepEqual(myCaves(db, ana, anaDev), []);
  assert.deepEqual(goneCaves(db, ana), ['cave-open1']);
  assert.deepEqual(deletedCaves(db, ana), []); // left, not deleted: the cave carries on for its owner
  assert.equal(entriesAfter(db, owner, 'cave-open1', 0, 10).length, 1);
});

test('an old database gains the deleted_at column', () => {
  const db = openDb(':memory:');
  const cols = (db.prepare('PRAGMA table_info(caves)').all() as { name: string }[]).map((c) => c.name);
  assert.ok(cols.includes('deleted_at'));
});

test('over HTTP: a guest cannot delete, leaves instead; the owner deletes and the listing says so', async () => {
  const db = openDb(':memory:');
  const deps: ApiDeps = {
    db, adminEmail: ADMIN, passKey: generateKeyPairSync('ed25519').privateKey, publicUrl: 'http://127.0.0.1', mailer: { url: '', key: '' },
    whoIs: async (r) => { const e = r.headers['x-test-user']; return e ? { email: String(e), name: String(e).split('@')[0], provider: null, subject: '' } : null; },
  };
  const server = createServer((req, res) => void handleApi(deps, req, res));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const call = async (user: string, method: string, path: string, body?: unknown, device = '') => {
    const res = await fetch(base + path, { method, headers: { 'x-test-user': user, 'Content-Type': 'application/json', ...(device ? { 'X-Cave-Device': device } : {}) }, body: body === undefined ? undefined : JSON.stringify(body) });
    return { status: res.status, json: await res.json() };
  };
  const ANA = 'ana@example.com';
  try {
    const dev = (await call(ADMIN, 'POST', '/api/devices', { publicKey: PUB(1) })).json.device.id;
    const admin = (await call(ADMIN, 'GET', '/api/me')).json.member.id;
    createInvite(db, admin, ANA, 'member', Date.now());
    const anaDev = (await call(ANA, 'POST', '/api/devices', { publicKey: PUB(2) })).json.device.id;
    await call(ADMIN, 'POST', '/api/caves', { id: 'cave-open1', storage: 'open' }, dev);
    await call(ADMIN, 'POST', '/api/caves', { id: 'cave-open2', storage: 'open' }, dev);
    const ana = ((await call(ADMIN, 'GET', '/api/members/directory')).json.members as { id: string; email: string }[]).find((m) => m.email === ANA)!.id;
    for (const c of ['cave-open1', 'cave-open2']) {
      const team = (await call(ADMIN, 'POST', `/api/caves/${c}/rooms`, { name: 'Team' }, dev)).json.id as string;
      await call(ADMIN, 'PUT', `/api/caves/${c}/rooms/${team}/guests/${ana}`, { seat: 'editor' }, dev);
    }
    assert.equal((await call(ANA, 'DELETE', '/api/caves/cave-open1', undefined, anaDev)).status, 404);
    assert.equal((await call(ADMIN, 'POST', '/api/caves/cave-open1/leave', undefined, dev)).status, 409);
    assert.equal((await call(ANA, 'POST', '/api/caves/cave-open2/leave', undefined, anaDev)).status, 200);
    assert.equal((await call(ADMIN, 'DELETE', '/api/caves/cave-open1', undefined, dev)).status, 200);
    const mine = (await call(ADMIN, 'GET', '/api/caves', undefined, dev)).json;
    assert.deepEqual(mine.caves.map((c: { id: string }) => c.id), ['cave-open2']);
    assert.deepEqual(mine.deleted, ['cave-open1']);
    const hers = (await call(ANA, 'GET', '/api/caves', undefined, anaDev)).json;
    assert.deepEqual(hers.caves, []);
    assert.deepEqual(hers.deleted, ['cave-open1']);
    assert.deepEqual(new Set(hers.gone), new Set(['cave-open1', 'cave-open2']));
    assert.equal((await call(ADMIN, 'POST', '/api/caves', { id: 'cave-open1', storage: 'open' }, dev)).status, 409);
  } finally {
    server.close();
  }
});
