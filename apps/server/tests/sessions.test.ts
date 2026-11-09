import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { openDb } from '../src/db.ts';
import { createSession, findSession, revokeSession, revokeAllFor, sessionCookie, sessionCookieName, clearSessionCookie, readCookie, SESSION_MS } from '../src/sessions.ts';

const who = { email: 'Rick@Example.com', name: 'Rick', provider: 'email' as const, subject: '' };
const now = 1_800_000_000_000;

test('a session finds its person until it expires', () => {
  const db = openDb(':memory:');
  const t = createSession(db, who, 'web', now);
  assert.equal(t.length, 43);
  const row = db.prepare('SELECT token_hash FROM sessions').get() as { token_hash: string };
  assert.equal(row.token_hash, createHash('sha256').update(t).digest('hex'));
  assert.deepEqual(findSession(db, t, now + 1000), { email: 'rick@example.com', name: 'Rick', provider: 'email', subject: 'session' });
  assert.equal(findSession(db, t, now + SESSION_MS + 1), null);
  assert.equal(findSession(db, 'nope', now), null);
  assert.equal(findSession(db, '', now), null);
});
test('revoking ends sessions', () => {
  const db = openDb(':memory:');
  const a = createSession(db, who, 'web', now);
  const b = createSession(db, who, 'desktop', now);
  revokeSession(db, a, now);
  assert.equal(findSession(db, a, now), null);
  assert.ok(findSession(db, b, now));
  revokeAllFor(db, 'RICK@example.com', now);
  assert.equal(findSession(db, b, now), null);
});
test('cookies', () => {
  assert.match(sessionCookie('tok', true), /^__Host-cavecad_session=tok; HttpOnly; Secure; SameSite=Lax; Path=\/; Max-Age=2592000$/);
  assert.ok(!sessionCookie('tok', false).includes('Secure'));
  assert.match(sessionCookie('tok', false), /^cavecad_session=tok;/);
  assert.equal(sessionCookieName(true), '__Host-cavecad_session');
  assert.equal(sessionCookieName(false), 'cavecad_session');
  assert.match(clearSessionCookie(true), /^__Host-cavecad_session=;/);
  assert.match(clearSessionCookie(true), /Max-Age=0/);
  assert.equal(readCookie('a=1; cavecad_session=xyz; b=2', 'cavecad_session'), 'xyz');
  assert.equal(readCookie(undefined, 'cavecad_session'), '');
});

test('a long-dead session row is cleaned up when a new one starts', () => {
  const db = openDb(':memory:');
  const who = { email: 'a@b.com', name: 'A', provider: 'google' as const, subject: 'x' };
  createSession(db, who, 'web', now - 3 * SESSION_MS);
  assert.equal((db.prepare('SELECT COUNT(*) AS n FROM sessions').get() as { n: number }).n, 1);
  createSession(db, who, 'web', now);
  assert.equal((db.prepare('SELECT COUNT(*) AS n FROM sessions').get() as { n: number }).n, 1);
});
test('a session can be asked for by kind', () => {
  const db = openDb(':memory:');
  const web = createSession(db, who, 'web', now);
  const desk = createSession(db, who, 'desktop', now);
  assert.ok(findSession(db, web, now, 'web'));
  assert.equal(findSession(db, web, now, 'desktop'), null);
  assert.ok(findSession(db, desk, now, 'desktop'));
  assert.equal(findSession(db, desk, now, 'web'), null);
  assert.ok(findSession(db, desk, now), 'no kind: either');
});
