import { test } from 'node:test';
import assert from 'node:assert/strict';
import { openDb } from '../src/db.ts';
import { resolve, createInvite } from '../src/members.ts';
import { openCave, putLabel, caveStorage, appendEntry, checkDevice, createCave, devicesMissingKey, entriesAfter, latestSnapshot, myCaves, putSnapshot, putWrappedKeys, registerDevice } from '../src/rooms.ts';

const T = 1_800_000_000_000;
function setup() {
  const db = openDb(':memory:');
  const a = resolve(db, { email: 'nathan@example.com', name: 'Nathan', provider: 'google', subject: 'a' }, { adminEmail: 'nathan@example.com', now: T });
  const owner = a.status === 'member' ? a.member.id : '';
  createInvite(db, owner, 'rick@example.com', 'member', T);
  const b = resolve(db, { email: 'rick@example.com', name: 'Rick', provider: 'google', subject: 'b' }, { adminEmail: 'nathan@example.com', now: T });
  const other = b.status === 'member' ? b.member.id : '';
  return { db, owner, other };
}

test('devices register once per key', () => {
  const { db, owner } = setup();
  const d1 = registerDevice(db, owner, 'PUB1'.padEnd(43, 'x'), T);
  assert.equal(registerDevice(db, owner, 'PUB1'.padEnd(43, 'x'), T + 1).id, d1.id);
  assert.notEqual(registerDevice(db, owner, 'PUB2'.padEnd(43, 'x'), T).id, d1.id);
});

test('a cave is made with its owner, Private room and wrapped keys', () => {
  const { db, owner } = setup();
  const laptop = registerDevice(db, owner, 'PUB1'.padEnd(43, 'x'), T);
  const tablet = registerDevice(db, owner, 'PUB2'.padEnd(43, 'x'), T);
  createCave(db, owner, 'cave-0001', [{ device: laptop.id, wrapped: 'W1' }], T);
  assert.throws(() => createCave(db, owner, 'cave-0001', [], T), /exists/);
  const mine = myCaves(db, owner, laptop.id);
  assert.equal(mine.length, 1);
  assert.equal(mine[0].id, 'cave-0001');
  assert.equal(mine[0].keyVersion, 1);
  assert.equal(mine[0].wrapped, 'W1');
  assert.equal(mine[0].privateRoom.length > 0, true);
  assert.equal(myCaves(db, owner, tablet.id)[0].wrapped, null);
  assert.deepEqual(devicesMissingKey(db, owner, 'cave-0001').map((d) => d.publicKey), ['PUB2'.padEnd(43, 'x')]);
  putWrappedKeys(db, owner, 'cave-0001', 1, [{ device: tablet.id, wrapped: 'W2' }]);
  assert.equal(myCaves(db, owner, tablet.id)[0].wrapped, 'W2');
  assert.equal(devicesMissingKey(db, owner, 'cave-0001').length, 0);
});

test('history is numbered per cave and read back in order', () => {
  const { db, owner } = setup();
  const d = registerDevice(db, owner, 'PUB1'.padEnd(43, 'x'), T);
  createCave(db, owner, 'cave-aaaa', [{ device: d.id, wrapped: 'W' }], T);
  assert.equal(appendEntry(db, owner, d.id, 'cave-aaaa', 1, 'AAA', T), 1);
  assert.equal(appendEntry(db, owner, d.id, 'cave-aaaa', 1, 'BBB', T), 2);
  assert.equal(appendEntry(db, owner, d.id, 'cave-aaaa', 1, 'CCC', T), 3);
  assert.deepEqual(entriesAfter(db, owner, 'cave-aaaa', 1, 10).map((e) => [e.number, e.locked]), [[2, 'BBB'], [3, 'CCC']]);
  assert.deepEqual(entriesAfter(db, owner, 'cave-aaaa', 0, 2).map((e) => e.number), [1, 2]);
  assert.equal(entriesAfter(db, owner, 'cave-aaaa', 0, 10)[0].authorDevice, d.id);
});

test('snapshots keep the latest', () => {
  const { db, owner } = setup();
  const d = registerDevice(db, owner, 'PUB1'.padEnd(43, 'x'), T);
  createCave(db, owner, 'cave-aaaa', [{ device: d.id, wrapped: 'W' }], T);
  assert.equal(latestSnapshot(db, owner, 'cave-aaaa'), null);
  for (let i = 0; i < 9; i++) appendEntry(db, owner, d.id, 'cave-aaaa', 1, 'E', T);
  putSnapshot(db, owner, 'cave-aaaa', 5, 1, 'SNAP5', T);
  putSnapshot(db, owner, 'cave-aaaa', 9, 1, 'SNAP9', T);
  assert.deepEqual(latestSnapshot(db, owner, 'cave-aaaa'), { upTo: 9, keyVersion: 1, locked: 'SNAP9' });
});

test('someone else gets not-found everywhere', () => {
  const { db, owner, other } = setup();
  const d = registerDevice(db, owner, 'PUB1'.padEnd(43, 'x'), T);
  const od = registerDevice(db, other, 'PUB9'.padEnd(43, 'x'), T);
  createCave(db, owner, 'cave-aaaa', [{ device: d.id, wrapped: 'W' }], T);
  assert.equal(myCaves(db, other, od.id).length, 0);
  assert.throws(() => appendEntry(db, other, od.id, 'cave-aaaa', 1, 'X', T), /not-found/);
  assert.throws(() => entriesAfter(db, other, 'cave-aaaa', 0, 10), /not-found/);
  assert.throws(() => putSnapshot(db, other, 'cave-aaaa', 1, 1, 'X', T), /not-found/);
  assert.throws(() => putWrappedKeys(db, other, 'cave-aaaa', 1, [{ device: od.id, wrapped: 'X' }]), /not-found/);
  assert.throws(() => devicesMissingKey(db, other, 'cave-aaaa'), /not-found/);
});

test('wrapped keys go only to the owner\'s own devices, and a device must be the caller\'s', () => {
  const { db, owner, other } = setup();
  const d = registerDevice(db, owner, 'PUB1'.padEnd(43, 'x'), T);
  const od = registerDevice(db, other, 'PUB9'.padEnd(43, 'x'), T);
  createCave(db, owner, 'cave-aaaa', [{ device: d.id, wrapped: 'W' }], T);
  assert.throws(() => putWrappedKeys(db, owner, 'cave-aaaa', 1, [{ device: od.id, wrapped: 'X' }]), /bad-device/);
  assert.throws(() => appendEntry(db, owner, od.id, 'cave-aaaa', 1, 'X', T), /bad-device/);
  assert.throws(() => createCave(db, owner, 'cave-bbbb', [{ device: od.id, wrapped: 'X' }], T), /bad-device/);
});

test('snapshots never go backwards, and never past the last entry', () => {
  const { db, owner } = setup();
  const d = registerDevice(db, owner, 'PUB1'.padEnd(43, 'x'), T);
  createCave(db, owner, 'cave-aaaa', [{ device: d.id, wrapped: 'W' }], T);
  putSnapshot(db, owner, 'cave-aaaa', 0, 1, 'SNAP0', T); // an empty cave may be snapshotted at 0
  assert.throws(() => putSnapshot(db, owner, 'cave-aaaa', 1, 1, 'X', T), /bad-request/);
  for (let i = 0; i < 9; i++) appendEntry(db, owner, d.id, 'cave-aaaa', 1, 'E', T);
  assert.throws(() => putSnapshot(db, owner, 'cave-aaaa', 10, 1, 'X', T), /bad-request/);
  putSnapshot(db, owner, 'cave-aaaa', 9, 1, 'SNAP9', T);
  putSnapshot(db, owner, 'cave-aaaa', 5, 1, 'SNAP5', T);
  putSnapshot(db, owner, 'cave-aaaa', 9, 1, 'SNAP9-again', T);
  assert.deepEqual(latestSnapshot(db, owner, 'cave-aaaa'), { upTo: 9, keyVersion: 1, locked: 'SNAP9' });
});

test('keys and entries only for the cave\'s current key version', () => {
  const { db, owner } = setup();
  const d = registerDevice(db, owner, 'PUB1'.padEnd(43, 'x'), T);
  const t = registerDevice(db, owner, 'PUB2'.padEnd(43, 'x'), T);
  createCave(db, owner, 'cave-aaaa', [{ device: d.id, wrapped: 'W' }], T);
  assert.throws(() => putWrappedKeys(db, owner, 'cave-aaaa', 2, [{ device: t.id, wrapped: 'W2' }]), /bad-request/);
  assert.throws(() => putWrappedKeys(db, owner, 'cave-aaaa', 0, [{ device: t.id, wrapped: 'W2' }]), /bad-request/);
  assert.throws(() => appendEntry(db, owner, d.id, 'cave-aaaa', 2, 'X', T), /bad-request/);
  assert.equal(entriesAfter(db, owner, 'cave-aaaa', 0, 10).length, 0);
});

test('catch-up batches stop at a byte budget', () => {
  const { db, owner } = setup();
  const d = registerDevice(db, owner, 'PUB1'.padEnd(43, 'x'), T);
  createCave(db, owner, 'cave-aaaa', [{ device: d.id, wrapped: 'W' }], T);
  for (let i = 0; i < 5; i++) appendEntry(db, owner, d.id, 'cave-aaaa', 1, 'A'.repeat(100), T);
  assert.deepEqual(entriesAfter(db, owner, 'cave-aaaa', 0, 500, 250).map((e) => e.number), [1, 2, 3]);
  assert.deepEqual(entriesAfter(db, owner, 'cave-aaaa', 0, 500, 1).map((e) => e.number), [1], 'always at least one');
});

test('the same public key may belong to two members, as two devices', () => {
  const { db, owner, other } = setup();
  const mine = registerDevice(db, owner, 'SAME'.padEnd(43, 'x'), T);
  const theirs = registerDevice(db, other, 'SAME'.padEnd(43, 'x'), T);
  assert.notEqual(mine.id, theirs.id);
  assert.equal(theirs.memberId, other);
  assert.equal(registerDevice(db, owner, 'SAME'.padEnd(43, 'x'), T + 1).id, mine.id);
  assert.equal(registerDevice(db, other, 'SAME'.padEnd(43, 'x'), T + 1).id, theirs.id);
});

test('checkDevice accepts only the caller\'s own device and marks it seen', () => {
  const { db, owner, other } = setup();
  const d = registerDevice(db, owner, 'PUB1'.padEnd(43, 'x'), T);
  const od = registerDevice(db, other, 'PUB9'.padEnd(43, 'x'), T);
  checkDevice(db, owner, d.id, T + 500);
  assert.equal((db.prepare('SELECT last_seen_at AS t FROM devices WHERE id = ?').get(d.id) as { t: number }).t, T + 500);
  assert.throws(() => checkDevice(db, owner, od.id, T), /bad-device/);
  assert.throws(() => checkDevice(db, owner, 'made-up', T), /bad-device/);
});

test('a device public key must be exactly 32 bytes (43 base64url characters)', () => {
  const { db, owner } = setup();
  for (const bad of ['A'.repeat(42), 'A'.repeat(44), 'A'.repeat(20), 'A'.repeat(42) + '=', 'A'.repeat(42) + '+']) {
    assert.throws(() => registerDevice(db, owner, bad, T), /bad-key/, bad);
  }
  assert.ok(registerDevice(db, owner, 'A'.repeat(43), T).id);
});

test('a snapshot asked for "after" a number is null unless it covers more', () => {
  const { db, owner } = setup();
  const d = registerDevice(db, owner, 'PUB1'.padEnd(43, 'x'), T);
  createCave(db, owner, 'cave-aaaa', [{ device: d.id, wrapped: 'W' }], T);
  for (let i = 0; i < 9; i++) appendEntry(db, owner, d.id, 'cave-aaaa', 1, 'E', T);
  putSnapshot(db, owner, 'cave-aaaa', 5, 1, 'SNAP5', T);
  assert.equal(latestSnapshot(db, owner, 'cave-aaaa', 5), null);
  assert.equal(latestSnapshot(db, owner, 'cave-aaaa', 7), null);
  assert.equal(latestSnapshot(db, owner, 'cave-aaaa', 4)?.upTo, 5);
  assert.equal(latestSnapshot(db, owner, 'cave-aaaa')?.upTo, 5);
});

const PLAIN = '{"ops":"[{\\"op\\":\\"add\\"}]","entrance":"37.123,-83.456"}';

test('an open cave stores plain text for entries, snapshot and label, with no keys', () => {
  const { db, owner } = setup();
  const d = registerDevice(db, owner, 'PUB1'.padEnd(43, 'x'), T);
  assert.throws(() => createCave(db, owner, 'cave-open1', [{ device: d.id, wrapped: 'W' }], T, 'open'), /bad-request/);
  createCave(db, owner, 'cave-open1', [], T, 'open');
  assert.equal(caveStorage(db, owner, 'cave-open1'), 'open');
  assert.equal(appendEntry(db, owner, d.id, 'cave-open1', 0, PLAIN, T), 1);
  assert.equal(appendEntry(db, owner, d.id, 'cave-open1', 0, PLAIN, T), 2);
  assert.equal(entriesAfter(db, owner, 'cave-open1', 0, 10)[0].locked, PLAIN);
  putSnapshot(db, owner, 'cave-open1', 2, 0, PLAIN, T);
  assert.equal(latestSnapshot(db, owner, 'cave-open1')?.locked, PLAIN);
  putLabel(db, owner, 'cave-open1', 0, '{"name":"X"}', T);
  const c = myCaves(db, owner, d.id)[0];
  assert.equal(c.label?.locked, '{"name":"X"}');
  assert.equal(c.wrapped, null);
  assert.equal(c.needsKeys, false);
  assert.throws(() => putWrappedKeys(db, owner, 'cave-open1', 1, []), /bad-request/);
  assert.equal(devicesMissingKey(db, owner, 'cave-open1').length, 0);
});

test('a locked cave stays locked by default and keeps its checks', () => {
  const { db, owner } = setup();
  const d = registerDevice(db, owner, 'PUB1'.padEnd(43, 'x'), T);
  createCave(db, owner, 'cave-lock1', [{ device: d.id, wrapped: 'W' }], T);
  assert.equal(caveStorage(db, owner, 'cave-lock1'), 'locked');
  assert.throws(() => appendEntry(db, owner, d.id, 'cave-lock1', 2, 'AAA', T), /bad-request/);
  assert.equal(myCaves(db, owner, d.id)[0].keyVersion, 1);
});

test('converting a locked cave to open drops the locked data and keeps numbering', () => {
  const { db, owner, other } = setup();
  const d = registerDevice(db, owner, 'PUB1'.padEnd(43, 'x'), T);
  createCave(db, owner, 'cave-conv1', [{ device: d.id, wrapped: 'W' }], T);
  for (const t of ['A', 'B', 'C']) appendEntry(db, owner, d.id, 'cave-conv1', 1, t, T);
  putSnapshot(db, owner, 'cave-conv1', 2, 1, 'SNAP', T);
  putLabel(db, owner, 'cave-conv1', 1, 'LABEL', T);
  assert.throws(() => openCave(db, other, 'cave-conv1', 3, PLAIN, T), /not-found/);
  assert.throws(() => openCave(db, owner, 'cave-conv1', 2, PLAIN, T), /bad-request/);
  assert.throws(() => openCave(db, owner, 'cave-conv1', 4, PLAIN, T), /bad-request/);
  openCave(db, owner, 'cave-conv1', 3, PLAIN, T);
  assert.equal(caveStorage(db, owner, 'cave-conv1'), 'open');
  assert.equal(entriesAfter(db, owner, 'cave-conv1', 0, 10).length, 0);
  const snap = latestSnapshot(db, owner, 'cave-conv1');
  assert.deepEqual([snap?.upTo, snap?.locked], [3, PLAIN]);
  const c = myCaves(db, owner, d.id)[0];
  assert.equal(c.wrapped, null);
  assert.equal(c.label, null);
  assert.equal(c.needsKeys, false);
  assert.equal((db.prepare('SELECT COUNT(*) AS n FROM wrapped_keys').get() as { n: number }).n, 0);
  assert.equal(appendEntry(db, owner, d.id, 'cave-conv1', 0, PLAIN, T), 4);
  assert.throws(() => openCave(db, owner, 'cave-conv1', 4, PLAIN, T), /bad-request/);
});

test('an existing database without the storage column gains it on start', async () => {
  const { DatabaseSync } = await import('node:sqlite');
  const { mkdtempSync } = await import('node:fs');
  const { tmpdir } = await import('node:os');
  const path = mkdtempSync(tmpdir() + '/cave-db-') + '/old.db';
  const old = new DatabaseSync(path);
  old.exec('CREATE TABLE caves (id TEXT PRIMARY KEY, key_version INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL); INSERT INTO caves (id, created_at) VALUES (\'old-cave1\', 1);');
  old.close();
  const db = openDb(path);
  assert.equal((db.prepare("SELECT storage FROM caves WHERE id = 'old-cave1'").get() as { storage: string }).storage, 'locked');
  db.close();
});

test('a writer holding a stale "locked" view is refused once the cave is open, never stored as plain text', () => {
  const { db, owner } = setup();
  const d = registerDevice(db, owner, 'PUB1'.padEnd(43, 'x'), T);
  createCave(db, owner, 'cave-race1', [{ device: d.id, wrapped: 'W' }], T);
  appendEntry(db, owner, d.id, 'cave-race1', 1, 'AAA', T);
  // A request decided "locked" earlier; the cave converts before its write lands.
  openCave(db, owner, 'cave-race1', 1, PLAIN, T);
  assert.throws(() => appendEntry(db, owner, d.id, 'cave-race1', 1, 'LOCKEDTEXT', T), /bad-request/);
  assert.throws(() => putLabel(db, owner, 'cave-race1', 1, 'LOCKEDLABEL', T), /bad-request/);
  assert.throws(() => putSnapshot(db, owner, 'cave-race1', 1, 1, 'LOCKEDSNAP', T), /bad-request/);
  const stored = (db.prepare("SELECT locked FROM history WHERE cave_id = 'cave-race1' UNION ALL SELECT locked FROM cave_labels WHERE cave_id = 'cave-race1' UNION ALL SELECT locked FROM snapshots WHERE cave_id = 'cave-race1'").all() as { locked: string }[]).map((r) => r.locked);
  assert.deepEqual(stored, [PLAIN]);
  // and the other way: a locked cave takes no version 0
  createCave(db, owner, 'cave-race2', [{ device: d.id, wrapped: 'W' }], T);
  assert.throws(() => appendEntry(db, owner, d.id, 'cave-race2', 0, PLAIN, T), /bad-request/);
  assert.throws(() => putLabel(db, owner, 'cave-race2', 0, PLAIN, T), /bad-request/);
  assert.throws(() => putSnapshot(db, owner, 'cave-race2', 0, 0, PLAIN, T), /bad-request/);
});
