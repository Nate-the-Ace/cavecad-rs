import { test } from 'node:test';
import assert from 'node:assert/strict';
import { openDb } from '../src/db.ts';
import { resolve, createInvite } from '../src/members.ts';
import { appendEntry, createCave, devicesMissingKey, entriesAfter, latestSnapshot, myCaves, putSnapshot, putWrappedKeys, registerDevice } from '../src/rooms.ts';

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
