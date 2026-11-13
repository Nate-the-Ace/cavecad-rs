// Settings that follow a member to every device: kept per member, a newer copy is never replaced by an older one.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { openDb } from '../src/db.ts';
import { createInvite, resolve } from '../src/members.ts';
import { getPref, putPref } from '../src/prefs.ts';

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
  return { db, owner, ana: join('ana@example.com', 'Ana') };
}

test('saved layouts follow the member, and an older copy never wins', () => {
  const { db, owner, ana } = people();
  assert.equal(getPref(db, owner, 'layouts'), null);
  putPref(db, owner, 'layouts', { slots: [1] }, T + 10, T + 20);
  assert.deepEqual(getPref(db, owner, 'layouts'), { value: { slots: [1] }, updatedAt: T + 10 });
  const kept = putPref(db, owner, 'layouts', { slots: [0] }, T + 5, T + 30);
  assert.deepEqual(kept?.value, { slots: [1] }, 'a device that was offline does not overwrite a newer copy');
  putPref(db, owner, 'layouts', { slots: [2] }, T + 40, T + 40);
  assert.deepEqual(getPref(db, owner, 'layouts')?.value, { slots: [2] });
  assert.equal(getPref(db, ana, 'layouts'), null, 'not anyone else’s');
  assert.throws(() => getPref(db, owner, 'theme'), /not-found/);
  putPref(db, owner, 'settings', { 'cavecad.units': 'imperial' }, T, T);
  putPref(db, owner, 'dock', { grid: {} }, T, T);
  assert.deepEqual(getPref(db, owner, 'settings')?.value, { 'cavecad.units': 'imperial' });
  assert.throws(() => putPref(db, owner, 'layouts', 'x'.repeat(300 * 1024), T, T), /too-big/);
});
