import { test } from 'node:test';
import assert from 'node:assert/strict';
import { openDb, setSetting } from '../src/db.ts';
import { addDrive, cancelInvite, createInvite, hashToken, listDrives, listInvites, listMembers, removeDrive, removeMember, resendInvite, resolve, setRole, type Identity } from '../src/members.ts';

const DAY = 86_400_000;
const T0 = 1_800_000_000_000;
const who = (email: string, name = email.split('@')[0]): Identity => ({ email, name, provider: 'google', subject: 's-' + email });
const ADMIN = 'nathan@example.com';

function setup() {
  const db = openDb(':memory:');
  const first = resolve(db, who(ADMIN, 'Nathan'), { adminEmail: ADMIN, now: T0 });
  assert.equal(first.status, 'member');
  return { db, admin: (first as { member: { id: string } }).member.id };
}

test('the first admin is made from ADMIN_EMAIL', () => {
  const { db } = setup();
  const [m] = listMembers(db);
  assert.equal(m.role, 'admin');
  assert.equal(m.email, ADMIN);
});

test('someone else is not let in without an invite', () => {
  const { db } = setup();
  const r = resolve(db, who('stranger@example.com'), { adminEmail: ADMIN, now: T0 });
  assert.deepEqual(r, { status: 'not-invited', email: 'stranger@example.com', reason: 'none' });
});

test('an invite lets that email in once, with its role', () => {
  const { db, admin } = setup();
  const { token } = createInvite(db, admin, 'Rick@Example.com', 'member', T0);
  const r = resolve(db, who('rick@example.com'), { adminEmail: ADMIN, now: T0 + DAY, token });
  assert.equal(r.status, 'member');
  assert.equal(r.status === 'member' && r.firstVisit, true);
  assert.equal(listInvites(db, T0 + DAY).length, 0);
  const again = resolve(db, who('rick@example.com'), { adminEmail: ADMIN, now: T0 + 2 * DAY });
  assert.equal(again.status, 'member');
});

test('the invite is found by email even without the link', () => {
  const { db, admin } = setup();
  createInvite(db, admin, 'iris@example.com', 'admin', T0);
  const r = resolve(db, who('iris@example.com'), { adminEmail: ADMIN, now: T0 + 1 });
  assert.equal(r.status === 'member' && r.member.role, 'admin');
});

test('a bad link says why', () => {
  const { db, admin } = setup();
  const expired = createInvite(db, admin, 'a@example.com', 'member', T0).token;
  assert.deepEqual(resolve(db, who('a@example.com'), { adminEmail: ADMIN, now: T0 + 8 * DAY, token: expired }), { status: 'not-invited', email: 'a@example.com', reason: 'expired', inviter: 'Nathan' });
  const other = createInvite(db, admin, 'b@example.com', 'member', T0).token;
  assert.equal((resolve(db, who('c@example.com'), { adminEmail: ADMIN, now: T0, token: other }) as { reason: string }).reason, 'other-email');
  const cancelled = createInvite(db, admin, 'd@example.com', 'member', T0);
  cancelInvite(db, admin, cancelled.invite.id, T0);
  assert.equal((resolve(db, who('d@example.com'), { adminEmail: ADMIN, now: T0, token: cancelled.token }) as { reason: string }).reason, 'cancelled');
  assert.equal((resolve(db, who('x@example.com'), { adminEmail: ADMIN, now: T0, token: 'nope' }) as { reason: string }).reason, 'none');
});

test('a used link from another account says used', () => {
  const { db, admin } = setup();
  const { token } = createInvite(db, admin, 'e@example.com', 'member', T0);
  resolve(db, who('e@example.com'), { adminEmail: ADMIN, now: T0, token });
  assert.equal((resolve(db, who('f@example.com'), { adminEmail: ADMIN, now: T0, token }) as { reason: string }).reason, 'used');
});

test('open sign-ups let anyone in as a member', () => {
  const { db } = setup();
  setSetting(db, 'signups', 'open');
  const r = resolve(db, who('anyone@example.com'), { adminEmail: ADMIN, now: T0 });
  assert.equal(r.status === 'member' && r.member.role, 'member');
});

test('a removed member is refused, and a new invite brings them back', () => {
  const { db, admin } = setup();
  createInvite(db, admin, 'g@example.com', 'member', T0);
  const g = resolve(db, who('g@example.com'), { adminEmail: ADMIN, now: T0 });
  const gid = g.status === 'member' ? g.member.id : '';
  removeMember(db, admin, gid, T0);
  assert.deepEqual(resolve(db, who('g@example.com'), { adminEmail: ADMIN, now: T0 }), { status: 'removed', email: 'g@example.com' });
  createInvite(db, admin, 'g@example.com', 'member', T0 + 1);
  assert.equal(resolve(db, who('g@example.com'), { adminEmail: ADMIN, now: T0 + 2 }).status, 'member');
});

test('admin rules', () => {
  const { db, admin } = setup();
  createInvite(db, admin, 'm@example.com', 'member', T0);
  const m = resolve(db, who('m@example.com'), { adminEmail: ADMIN, now: T0 });
  const mid = m.status === 'member' ? m.member.id : '';
  assert.throws(() => createInvite(db, mid, 'z@example.com', 'member', T0), /not-admin/);
  assert.throws(() => setRole(db, mid, admin, 'member'), /not-admin/);
  assert.throws(() => setRole(db, admin, admin, 'member'), /self/);
  assert.throws(() => removeMember(db, admin, admin, T0), /self/);
  assert.throws(() => createInvite(db, admin, 'm@example.com', 'member', T0), /already-member/);
  setRole(db, admin, mid, 'admin');
  setRole(db, mid, admin, 'member'); // two admins: one may demote the other
  assert.throws(() => removeMember(db, admin, mid, T0), /not-admin/);
});

test('the last admin cannot be demoted or removed', () => {
  const { db, admin } = setup();
  createInvite(db, admin, 'n@example.com', 'admin', T0);
  const n = resolve(db, who('n@example.com'), { adminEmail: ADMIN, now: T0 });
  const nid = n.status === 'member' ? n.member.id : '';
  removeMember(db, nid, admin, T0); // nathan removed by the other admin
  assert.throws(() => setRole(db, nid, nid, 'member'), /self/);
  assert.equal(listMembers(db).filter((x) => x.role === 'admin').length, 1);
});

test('tokens are stored only as hashes; resend makes a new one', () => {
  const { db, admin } = setup();
  const { invite, token } = createInvite(db, admin, 'h@example.com', 'member', T0);
  const raw = db.prepare('SELECT token_hash FROM invites WHERE id = ?').get(invite.id) as { token_hash: string };
  assert.equal(raw.token_hash, hashToken(token));
  assert.notEqual(raw.token_hash, token);
  const again = resendInvite(db, admin, invite.id, T0 + DAY);
  assert.notEqual(again.token, token);
  assert.equal(again.invite.expiresAt, T0 + 8 * DAY);
});

test('drives belong to one member', () => {
  const { db, admin } = setup();
  createInvite(db, admin, 'k@example.com', 'member', T0);
  const k = resolve(db, who('k@example.com'), { adminEmail: ADMIN, now: T0 });
  const kid = k.status === 'member' ? k.member.id : '';
  const d = addDrive(db, admin, 'gdrive', 'nathan@example.com', T0);
  assert.equal(listDrives(db, admin).length, 1);
  assert.throws(() => removeDrive(db, kid, d.id), /not-found/);
  removeDrive(db, admin, d.id);
  assert.equal(listDrives(db, admin).length, 0);
  assert.throws(() => addDrive(db, admin, 'icloud' as 'gdrive', '', T0), /bad-provider/);
});
