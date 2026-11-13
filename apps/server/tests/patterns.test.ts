// A member's pattern library and pattern shares: kept per member, replaced by name, shared as a snapshot.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { openDb } from '../src/db.ts';
import { createInvite, resolve } from '../src/members.ts';
import { listPatterns, putPattern, readShare, removePattern, sharePatterns, MAX_DEF_BYTES } from '../src/patterns.ts';

const T = 1_800_000_000_000;
const ADMIN = 'nathan@example.com';

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
  return { db, owner, ana };
}

const line = (n: number) => ({ elements: [{ t: 'dash', mm: n }, { t: 'gap', mm: 1 }] });

test('each member keeps their own patterns, replaced by kind and name', () => {
  const { db, owner, ana } = people();
  putPattern(db, owner, 'line', 'Mine', line(2), T);
  putPattern(db, owner, 'line', 'Mine', line(3), T + 1);
  putPattern(db, owner, 'fill', 'Mine', { layers: [], tint: true }, T);
  const mine = listPatterns(db, owner);
  assert.equal(mine.length, 2, 'a line and a fill may share a name');
  const l = mine.find((p) => p.kind === 'line')!;
  assert.deepEqual((l.def as { elements: unknown[] }).elements[0], { t: 'dash', mm: 3 });
  assert.equal((l.def as { name: string }).name, 'Mine', 'the stored definition carries its name');
  assert.equal(listPatterns(db, ana).length, 0, 'not shown to anyone else');
  removePattern(db, owner, 'line', 'Mine');
  assert.deepEqual(listPatterns(db, owner).map((p) => p.kind), ['fill']);
});

test('bad patterns are refused', () => {
  const { db, owner } = people();
  assert.throws(() => putPattern(db, owner, 'brush', 'X', line(1), T), /bad-kind/);
  assert.throws(() => putPattern(db, owner, 'line', '', line(1), T), /bad-name/);
  assert.throws(() => putPattern(db, owner, 'line', 'x'.repeat(41), line(1), T), /bad-name/);
  assert.throws(() => putPattern(db, owner, 'line', 'X', [1, 2], T), /bad-def/);
  assert.throws(() => putPattern(db, owner, 'line', 'X', { big: 'y'.repeat(MAX_DEF_BYTES) }, T), /too-big/);
});

test('a share is a snapshot anyone signed in can read; later edits do not change it', () => {
  const { db, owner, ana } = people();
  putPattern(db, owner, 'line', 'Wall', line(2), T);
  putPattern(db, owner, 'motif', 'Rock', { prims: [] }, T);
  const code = sharePatterns(db, owner, [{ kind: 'line', name: 'Wall' }, { kind: 'motif', name: 'Rock' }], T);
  putPattern(db, owner, 'line', 'Wall', line(9), T + 1);
  const got = readShare(db, code);
  assert.equal(got.from, 'Nathan');
  assert.deepEqual(got.items.map((i) => [i.kind, i.name]), [['line', 'Wall'], ['motif', 'Rock']]);
  assert.deepEqual((got.items[0].def as { elements: unknown[] }).elements[0], { t: 'dash', mm: 2 }, 'the copy as it was shared');
  assert.throws(() => sharePatterns(db, ana, [{ kind: 'line', name: 'Wall' }], T), /not-found/, 'only your own patterns can be shared');
  assert.throws(() => readShare(db, 'nope-nope-nope'), /not-found/);
  assert.throws(() => sharePatterns(db, owner, [], T), /bad-request/);
});
