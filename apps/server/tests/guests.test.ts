// Rooms and guest lists: owners change them, guests read (editors also write), removal stops a guest at once.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { generateKeyPairSync } from 'node:crypto';
import { openDb } from '../src/db.ts';
import { handleApi, type ApiDeps } from '../src/api.ts';
import { listeners } from '../src/streams.ts';
import { createInvite, resolve, removeMember } from '../src/members.ts';
import { accessOf, addRoom, appendEntry, caveRooms, createCave, entriesAfter, goneCaves, myCaves, registerDevice, removeGuest, removeRoom, renameRoom, setGuest } from '../src/rooms.ts';

const T = 1_800_000_000_000;
const ADMIN = 'nathan@example.com';
const PUB = (n: number) => ('P'.repeat(30) + n).padEnd(43, 'x');

function people() {
  const db = openDb(':memory:');
  const join = (email: string, name: string) => {
    if (email !== ADMIN) createInvite(db, ownerId, email, 'member', T);
    const r = resolve(db, { email, name, provider: 'google', subject: email }, { adminEmail: ADMIN, now: T });
    return r.status === 'member' ? r.member.id : '';
  };
  let ownerId = '';
  ownerId = join(ADMIN, 'Nathan');
  const ana = join('ana@example.com', 'Ana');
  const ben = join('ben@example.com', 'Ben');
  const dev = registerDevice(db, ownerId, PUB(1), T).id;
  const anaDev = registerDevice(db, ana, PUB(2), T).id;
  createCave(db, ownerId, 'cave-open1', [], T, 'open');
  return { db, owner: ownerId, ana, ben, dev, anaDev };
}

test('only owners add, rename and remove rooms; the Private room stays', () => {
  const { db, owner, ana } = people();
  assert.throws(() => addRoom(db, ana, 'cave-open1', 'Team', T), /not-found/);
  const team = addRoom(db, owner, 'cave-open1', '  Survey   team ', T);
  assert.throws(() => addRoom(db, owner, 'cave-open1', 'survey team', T), /name-taken/);
  assert.throws(() => addRoom(db, owner, 'cave-open1', 'private', T), /name-taken/);
  assert.throws(() => addRoom(db, owner, 'cave-open1', '   ', T), /bad-name/);
  assert.throws(() => addRoom(db, owner, 'cave-open1', 'x'.repeat(61), T), /bad-name/);
  const rooms = caveRooms(db, owner, 'cave-open1').rooms;
  assert.deepEqual(rooms.map((r) => [r.name, r.kind]), [['Private', 'private'], ['Survey team', 'shared']]);
  const priv = rooms[0].id;
  assert.throws(() => renameRoom(db, owner, 'cave-open1', priv, 'Mine'), /private/);
  assert.throws(() => removeRoom(db, owner, 'cave-open1', priv, T), /private/);
  assert.throws(() => setGuest(db, owner, 'cave-open1', priv, ana, 'editor', T), /private/);
  assert.throws(() => renameRoom(db, ana, 'cave-open1', team, 'Mine'), /not-found/);
  renameRoom(db, owner, 'cave-open1', team, 'Spring class');
  assert.equal(caveRooms(db, owner, 'cave-open1').rooms[1].name, 'Spring class');
  assert.throws(() => removeRoom(db, ana, 'cave-open1', team, T), /not-found/);
  removeRoom(db, owner, 'cave-open1', team, T);
  assert.equal(caveRooms(db, owner, 'cave-open1').rooms.length, 1);
});

test('guests: owner only, members only, never an owner, open caves only; the best seat wins', () => {
  const { db, owner, ana, ben, anaDev } = people();
  const team = addRoom(db, owner, 'cave-open1', 'Team', T);
  const cls = addRoom(db, owner, 'cave-open1', 'Class', T);
  assert.throws(() => setGuest(db, ana, 'cave-open1', team, ben, 'editor', T), /not-found/);
  assert.throws(() => setGuest(db, owner, 'cave-open1', team, owner, 'editor', T), /is-owner/);
  assert.throws(() => setGuest(db, owner, 'cave-open1', team, 'no-such-member', 'editor', T), /no-member/);
  assert.throws(() => setGuest(db, owner, 'cave-open1', team, ana, 'host', T), /bad-request/);
  assert.equal(accessOf(db, ana, 'cave-open1'), null);
  setGuest(db, owner, 'cave-open1', cls, ana, 'watcher', T);
  assert.equal(accessOf(db, ana, 'cave-open1'), 'watcher');
  setGuest(db, owner, 'cave-open1', team, ana, 'editor', T);
  assert.equal(accessOf(db, ana, 'cave-open1'), 'editor');
  setGuest(db, owner, 'cave-open1', team, ana, 'watcher', T); // a change of seat, not a second entry
  assert.equal(accessOf(db, ana, 'cave-open1'), 'watcher');
  // A guest sees only the rooms they are in, and the cave on their list as shared.
  setGuest(db, owner, 'cave-open1', team, ben, 'editor', T);
  removeGuest(db, owner, 'cave-open1', cls, ana, T);
  assert.deepEqual(caveRooms(db, ana, 'cave-open1').rooms.map((r) => r.name), ['Team']);
  assert.equal(caveRooms(db, ana, 'cave-open1').you, 'watcher');
  const listed = myCaves(db, ana, anaDev);
  assert.deepEqual(listed.map((c) => [c.id, c.role, c.ownerName]), [['cave-open1', 'watcher', 'Nathan']]);
  // A locked cave from before takes no guests until it is opened.
  createCave(db, owner, 'cave-lock1', [], T);
  const lockRoom = addRoom(db, owner, 'cave-lock1', 'Team', T);
  assert.throws(() => setGuest(db, owner, 'cave-lock1', lockRoom, ana, 'editor', T), /locked/);
});

test('editors write, watchers only read, strangers see nothing; removal marks the cave gone until added back', () => {
  const { db, owner, ana, ben, anaDev } = people();
  const team = addRoom(db, owner, 'cave-open1', 'Team', T);
  assert.throws(() => entriesAfter(db, ana, 'cave-open1', 0, 10), /not-found/);
  setGuest(db, owner, 'cave-open1', team, ana, 'editor', T);
  assert.equal(appendEntry(db, ana, anaDev, 'cave-open1', 0, '{"c":"a","ops":"[]"}', T), 1);
  assert.equal(entriesAfter(db, ana, 'cave-open1', 0, 10).length, 1);
  setGuest(db, owner, 'cave-open1', team, ana, 'watcher', T);
  assert.throws(() => appendEntry(db, ana, anaDev, 'cave-open1', 0, '{}', T), /watcher/);
  assert.equal(entriesAfter(db, ana, 'cave-open1', 0, 10).length, 1);
  assert.throws(() => entriesAfter(db, ben, 'cave-open1', 0, 10), /not-found/);
  assert.deepEqual(goneCaves(db, ana), []);
  removeGuest(db, owner, 'cave-open1', team, ana, T);
  assert.deepEqual(goneCaves(db, ana), ['cave-open1']);
  assert.deepEqual(myCaves(db, ana, anaDev), []);
  assert.throws(() => entriesAfter(db, ana, 'cave-open1', 0, 10), /not-found/);
  setGuest(db, owner, 'cave-open1', team, ana, 'editor', T);
  assert.deepEqual(goneCaves(db, ana), []);
  // Removing a room takes its guests with it.
  removeRoom(db, owner, 'cave-open1', team, T);
  assert.deepEqual(goneCaves(db, ana), ['cave-open1']);
  // A guest still in another room is not gone.
  const a = addRoom(db, owner, 'cave-open1', 'A', T);
  const b = addRoom(db, owner, 'cave-open1', 'B', T);
  setGuest(db, owner, 'cave-open1', a, ana, 'editor', T);
  setGuest(db, owner, 'cave-open1', b, ana, 'watcher', T);
  removeGuest(db, owner, 'cave-open1', a, ana, T);
  assert.deepEqual(goneCaves(db, ana), []);
  assert.equal(accessOf(db, ana, 'cave-open1'), 'watcher');
  // A removed member cannot be added.
  removeMember(db, owner, ben, T);
  assert.throws(() => setGuest(db, owner, 'cave-open1', b, ben, 'editor', T), /no-member/);
});

async function start() {
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
  return { server, base, call, db };
}

test('over HTTP: the directory, guest list changes, watcher refusals, and streams closed on a seat change', async () => {
  const { server, base, call, db } = await start();
  const ANA = 'ana@example.com';
  try {
    const dev = (await call(ADMIN, 'POST', '/api/devices', { publicKey: PUB(1) })).json.device.id;
    const admin = (await call(ADMIN, 'GET', '/api/me')).json.member.id;
    createInvite(db, admin, ANA, 'member', Date.now());
    const anaDev = (await call(ANA, 'POST', '/api/devices', { publicKey: PUB(2) })).json.device.id;
    const dir = (await call(ANA, 'GET', '/api/members/directory')).json.members as { id: string; name: string; email: string }[];
    assert.deepEqual(dir.map((m) => m.email).sort(), [ANA, ADMIN]);
    const ana = dir.find((m) => m.email === ANA)!.id;
    await call(ADMIN, 'POST', '/api/caves', { id: 'cave-open1', storage: 'open' }, dev);
    assert.equal((await call(ANA, 'POST', '/api/caves/cave-open1/rooms', { name: 'Team' }, anaDev)).status, 404);
    const team = (await call(ADMIN, 'POST', '/api/caves/cave-open1/rooms', { name: 'Team' }, dev)).json.id as string;
    assert.equal((await call(ADMIN, 'POST', '/api/caves/cave-open1/rooms', { name: 'team' }, dev)).status, 409);
    assert.equal((await call(ADMIN, 'PUT', `/api/caves/cave-open1/rooms/${team}/guests/${ana}`, { seat: 'editor' }, dev)).status, 200);
    const seen = (await call(ANA, 'GET', '/api/caves/cave-open1/rooms', undefined, anaDev)).json;
    assert.equal(seen.you, 'editor');
    assert.deepEqual(seen.owners.map((o: { email: string }) => o.email), [ADMIN]);
    assert.equal((await call(ANA, 'PUT', `/api/caves/cave-open1/rooms/${team}/guests/${ana}`, { seat: 'editor' }, anaDev)).status, 404);
    assert.equal((await call(ANA, 'POST', '/api/caves/cave-open1/entries', { locked: '{"c":"a","ops":"[]"}' }, anaDev)).json.number, 1);
    const listed = (await call(ANA, 'GET', '/api/caves', undefined, anaDev)).json;
    assert.deepEqual([listed.caves.map((c: { id: string; role: string }) => [c.id, c.role]), listed.gone], [[['cave-open1', 'editor']], []]);
    // A seat change closes Ana's open stream, so her app reads its seat again.
    const res = await fetch(base + '/api/caves/cave-open1/stream?after=0', { headers: { 'x-test-user': ANA, 'X-Cave-Device': anaDev } });
    const ended = (async () => { const r = res.body!.getReader(); try { for (;;) if ((await r.read()).done) return 'ended'; } catch { return 'ended'; } })();
    await new Promise((r) => setTimeout(r, 50));
    assert.equal(listeners('cave-open1'), 1);
    await call(ADMIN, 'PUT', `/api/caves/cave-open1/rooms/${team}/guests/${ana}`, { seat: 'watcher' }, dev);
    assert.equal(await ended, 'ended');
    assert.equal(listeners('cave-open1'), 0);
    const refused = await call(ANA, 'POST', '/api/caves/cave-open1/entries', { locked: '{}' }, anaDev);
    assert.deepEqual([refused.status, refused.json.error], [403, 'watcher']);
    assert.equal((await call(ANA, 'PUT', '/api/caves/cave-open1/snapshot', { upTo: 1, locked: '{}' }, anaDev)).status, 403);
    assert.equal((await call(ANA, 'PUT', '/api/caves/cave-open1/label', { locked: '{}' }, anaDev)).status, 403);
    assert.equal((await call(ANA, 'GET', '/api/caves/cave-open1/snapshot', undefined, anaDev)).status, 200);
    // Taken off: the cave is listed as gone and nothing more is read.
    assert.equal((await call(ADMIN, 'DELETE', `/api/caves/cave-open1/rooms/${team}/guests/${ana}`, undefined, dev)).status, 200);
    const after = (await call(ANA, 'GET', '/api/caves', undefined, anaDev)).json;
    assert.deepEqual([after.caves, after.gone], [[], ['cave-open1']]);
    assert.equal((await call(ANA, 'GET', '/api/caves/cave-open1/snapshot', undefined, anaDev)).status, 404);
    assert.equal((await call(ADMIN, 'PATCH', `/api/caves/cave-open1/rooms/${team}`, { name: 'Spring' }, dev)).status, 200);
    assert.equal((await call(ADMIN, 'DELETE', `/api/caves/cave-open1/rooms/${team}`, undefined, dev)).status, 200);
    assert.equal((await call(ADMIN, 'GET', '/api/caves/cave-open1/rooms', undefined, dev)).json.rooms.length, 1);
  } finally {
    server.close();
  }
});
