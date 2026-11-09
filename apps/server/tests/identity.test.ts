import { test } from 'node:test';
import assert from 'node:assert/strict';
import { openDb } from '../src/db.ts';
import { createSession, revokeSession } from '../src/sessions.ts';
import { makeWhoIs, type RequestLike } from '../src/identity.ts';

const now = 1_800_000_000_000;
const req = (addr: string, headers: Record<string, string>): RequestLike => ({ socket: { remoteAddress: addr }, headers });
const who = { email: 'rick@example.com', name: 'Rick', provider: 'email' as const, subject: '' };

test('a session cookie or bearer token says who it is', async () => {
  const db = openDb(':memory:');
  const whoIs = makeWhoIs({ db, now: () => now });
  const web = createSession(db, who, 'web', now);
  const desk = createSession(db, who, 'desktop', now);
  assert.equal((await whoIs(req('1.2.3.4', { cookie: `x=1; cavecad_session=${web}` })))?.email, 'rick@example.com');
  assert.equal((await whoIs(req('1.2.3.4', { authorization: `Bearer ${desk}` })))?.email, 'rick@example.com');
  revokeSession(db, web, now);
  assert.equal(await whoIs(req('1.2.3.4', { cookie: `cavecad_session=${web}` })), null);
});
test('edge headers are no longer believed', async () => {
  const db = openDb(':memory:');
  const whoIs = makeWhoIs({ db, now: () => now });
  assert.equal(await whoIs(req('192.168.86.13', { 'x-cave-gate': 'anything', 'remote-email': 'rick@example.com' })), null);
});
test('tailnet and dev identities work from this machine only', async () => {
  const db = openDb(':memory:');
  const t = makeWhoIs({ db, tailnet: true, now: () => now });
  assert.equal((await t(req('127.0.0.1', { 'tailscale-user-login': 'Rick@Example.com', 'tailscale-user-name': '=?utf-8?q?Jos=C3=A9?=' })))?.name, 'José');
  assert.equal(await t(req('10.0.0.2', { 'tailscale-user-login': 'rick@example.com' })), null);
  const d = makeWhoIs({ db, devIdentity: 'dev@example.com', now: () => now });
  assert.equal((await d(req('::1', {})))?.email, 'dev@example.com');
  assert.equal(await d(req('10.0.0.2', {})), null);
});
test('a web cookie is not a bearer token, nor a desktop token a cookie', async () => {
  const db = openDb(':memory:');
  const whoIs = makeWhoIs({ db, now: () => now });
  const web = createSession(db, who, 'web', now);
  const desk = createSession(db, who, 'desktop', now);
  assert.equal(await whoIs(req('1.2.3.4', { authorization: `Bearer ${web}` })), null);
  assert.equal(await whoIs(req('1.2.3.4', { cookie: `cavecad_session=${desk}` })), null);
});
