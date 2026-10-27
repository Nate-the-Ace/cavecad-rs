// Rooms and guest lists: owners change them, guests read (editors also write), removal stops a guest at once.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { generateKeyPairSync } from 'node:crypto';
import { openDb } from '../src/db.ts';
import { handleApi, type ApiDeps } from '../src/api.ts';
import { listeners } from '../src/streams.ts';
import { cancelInvite, createInvite, listInvites, resolve, removeMember } from '../src/members.ts';
import { accessOf, addRoom, cancelInvited, inviteGuest, appendEntry, caveRooms, createCave, entriesAfter, goneCaves, myCaves, registerDevice, removeGuest, removeRoom, renameRoom, setGuest } from '../src/rooms.ts';

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
    // Only cave owners list members (and so see their emails).
    assert.equal((await call(ANA, 'GET', '/api/members/directory')).status, 403);
    assert.equal((await call(ADMIN, 'GET', '/api/members/directory')).status, 403);
    await call(ADMIN, 'POST', '/api/caves', { id: 'cave-open1', storage: 'open' }, dev);
    const dir = (await call(ADMIN, 'GET', '/api/members/directory')).json.members as { id: string; name: string; email: string }[];
    assert.deepEqual(dir.map((m) => m.email).sort(), [ANA, ADMIN]);
    const ana = dir.find((m) => m.email === ANA)!.id;
    assert.equal((await call(ANA, 'POST', '/api/caves/cave-open1/rooms', { name: 'Team' }, anaDev)).status, 404);
    const team = (await call(ADMIN, 'POST', '/api/caves/cave-open1/rooms', { name: 'Team' }, dev)).json.id as string;
    assert.equal((await call(ADMIN, 'POST', '/api/caves/cave-open1/rooms', { name: 'team' }, dev)).status, 409);
    assert.equal((await call(ADMIN, 'PUT', `/api/caves/cave-open1/rooms/${team}/guests/${ana}`, { seat: 'editor' }, dev)).status, 200);
    const seen = (await call(ANA, 'GET', '/api/caves/cave-open1/rooms', undefined, anaDev)).json;
    assert.equal(seen.you, 'editor');
    // A guest sees names, never email addresses; the owner sees both.
    assert.deepEqual(seen.owners.map((o: { name: string; email: string }) => [o.name, o.email]), [['nathan', '']]);
    assert.deepEqual(seen.rooms[0].guests.map((g: { email: string }) => g.email), ['']);
    assert.deepEqual((await call(ADMIN, 'GET', '/api/caves/cave-open1/rooms', undefined, dev)).json.rooms[1].guests.map((g: { email: string }) => g.email), [ANA]);
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

test('inviting by email: admins only; the kept seat becomes theirs when they join; cancel, expiry and room removal drop it', () => {
  const { db, owner, ana } = people();
  const team = addRoom(db, owner, 'cave-open1', 'Team', T);
  // A member's email seats them at once, whoever the owner is.
  assert.deepEqual(inviteGuest(db, owner, 'cave-open1', team, 'ANA@example.com ', 'editor', T), { memberId: ana });
  // Someone new: an owner who is not an admin is refused.
  createCave(db, ana, 'cave-ana01', [], T, 'open');
  const anaRoom = addRoom(db, ana, 'cave-ana01', 'Hers', T);
  assert.throws(() => inviteGuest(db, ana, 'cave-ana01', anaRoom, 'dan@example.com', 'editor', T), /admin-only/);
  assert.throws(() => inviteGuest(db, ana, 'cave-open1', team, 'dan@example.com', 'editor', T), /not-found/); // not her cave
  assert.throws(() => inviteGuest(db, owner, 'cave-open1', team, 'not an email', 'editor', T), /bad-email/);
  // The admin owner invites Dan: the seat waits, shown to owners as invited.
  assert.deepEqual(inviteGuest(db, owner, 'cave-open1', team, 'dan@example.com', 'watcher', T), { invited: 'dan@example.com' });
  createInvite(db, owner, 'dan@example.com', 'member', T);
  assert.deepEqual(caveRooms(db, owner, 'cave-open1', T).rooms[1].invited, [{ email: 'dan@example.com', seat: 'watcher' }]);
  assert.deepEqual(caveRooms(db, ana, 'cave-open1', T).rooms[0].invited, []); // guests never see invited emails
  // Dan joins through his invite: he is in the room with the kept seat.
  const dan = resolve(db, { email: 'dan@example.com', name: 'Dan', provider: 'google', subject: 'dan' }, { adminEmail: ADMIN, now: T + 1 });
  assert.equal(dan.status, 'member');
  const danId = dan.status === 'member' ? dan.member.id : '';
  assert.equal(accessOf(db, danId, 'cave-open1'), 'watcher');
  assert.deepEqual(caveRooms(db, owner, 'cave-open1', T).rooms[1].invited, []);
  // A cancelled invite drops its kept seats.
  inviteGuest(db, owner, 'cave-open1', team, 'eve@example.com', 'editor', T);
  const eveInvite = createInvite(db, owner, 'eve@example.com', 'member', T).invite;
  cancelInvite(db, owner, eveInvite.id, T);
  assert.equal((db.prepare("SELECT COUNT(*) AS n FROM room_pending WHERE email = 'eve@example.com'").get() as { n: number }).n, 0);
  // An expired invite shows no seat and seats no one.
  inviteGuest(db, owner, 'cave-open1', team, 'fay@example.com', 'editor', T);
  createInvite(db, owner, 'fay@example.com', 'member', T);
  const later = T + 30 * 86_400_000;
  assert.deepEqual(caveRooms(db, owner, 'cave-open1', later).rooms[1].invited, []);
  assert.equal(listInvites(db, later).length, 0);
  // The owner cancels a kept seat; removing the room drops the rest.
  inviteGuest(db, owner, 'cave-open1', team, 'gus@example.com', 'editor', T);
  createInvite(db, owner, 'gus@example.com', 'member', T);
  assert.throws(() => cancelInvited(db, ana, 'cave-open1', team, 'gus@example.com'), /not-found/);
  cancelInvited(db, owner, 'cave-open1', team, 'GUS@example.com');
  assert.deepEqual(caveRooms(db, owner, 'cave-open1', T).rooms[1].invited.map((p) => p.email), ['fay@example.com']); // Fay's is still live at T
  inviteGuest(db, owner, 'cave-open1', team, 'hal@example.com', 'editor', T);
  removeRoom(db, owner, 'cave-open1', team, T);
  assert.equal((db.prepare('SELECT COUNT(*) AS n FROM room_pending').get() as { n: number }).n, 0);
});

test('over HTTP: an admin owner invites by email and gets a link; a non-admin owner is refused', async () => {
  const { server, call, db } = await start();
  try {
    const dev = (await call(ADMIN, 'POST', '/api/devices', { publicKey: PUB(1) })).json.device.id;
    await call(ADMIN, 'POST', '/api/caves', { id: 'cave-open1', storage: 'open' }, dev);
    const team = (await call(ADMIN, 'POST', '/api/caves/cave-open1/rooms', { name: 'Team' }, dev)).json.id as string;
    const sent = await call(ADMIN, 'POST', `/api/caves/cave-open1/rooms/${team}/invited`, { email: 'dan@example.com', seat: 'editor' });
    assert.equal(sent.status, 200);
    assert.equal(sent.json.emailed, false); // no mailer in tests: the panel offers the link to copy
    assert.match(sent.json.link, /\?signup=/);
    const rooms = (await call(ADMIN, 'GET', '/api/caves/cave-open1/rooms', undefined, dev)).json.rooms;
    assert.deepEqual(rooms[1].invited, [{ email: 'dan@example.com', seat: 'editor' }]);
    assert.equal((await call(ADMIN, 'DELETE', `/api/caves/cave-open1/rooms/${team}/invited?email=dan%40example.com`)).status, 200);
    // Rick, a member who is not an admin, owns a cave and tries to invite someone new.
    const admin = (await call(ADMIN, 'GET', '/api/me')).json.member.id;
    createInvite(db, admin, 'rick@example.com', 'member', Date.now());
    const rdev = (await call('rick@example.com', 'POST', '/api/devices', { publicKey: PUB(3) })).json.device.id;
    await call('rick@example.com', 'POST', '/api/caves', { id: 'cave-rick1', storage: 'open' }, rdev);
    const rroom = (await call('rick@example.com', 'POST', '/api/caves/cave-rick1/rooms', { name: 'Team' }, rdev)).json.id as string;
    const refused = await call('rick@example.com', 'POST', `/api/caves/cave-rick1/rooms/${rroom}/invited`, { email: 'zed@example.com', seat: 'editor' });
    assert.deepEqual([refused.status, refused.json.error], [403, 'admin-only']);
    // ...but a member's email works for him.
    const ok = await call('rick@example.com', 'POST', `/api/caves/cave-rick1/rooms/${rroom}/invited`, { email: ADMIN, seat: 'watcher' });
    assert.equal(ok.status, 200);
    assert.ok(ok.json.added);
  } finally {
    server.close();
  }
});
