import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer, type IncomingMessage } from 'node:http';
import { createHash } from 'node:crypto';
import { openDb } from '../src/db.ts';
import { handleSignin, clientIp, type SigninDeps } from '../src/signin.ts';
import { findSession } from '../src/sessions.ts';
import { redeemHandoff } from '../src/handoff.ts';

async function start() {
  const db = openDb(':memory:');
  const sent: { to: string; code: string }[] = [];
  const d: SigninDeps = {
    db, publicUrl: 'https://cavecad.test', secure: true,
    codes: { db, secret: 'secret-0123456789abcdef', send: async (to, code) => (sent.push({ to, code }), true), mayGetIn: (e) => e === 'rick@example.com' },
    google: null,
  };
  const server = createServer((req, res) => void handleSignin(d, req, res).then((done) => { if (!done) res.writeHead(404).end(); }));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const post = (path: string, body: object, h: Record<string, string> = {}) => fetch(base + path, { method: 'POST', headers: { 'Content-Type': 'application/json', 'Sec-Fetch-Site': 'same-origin', ...h }, body: JSON.stringify(body), redirect: 'manual' });
  return { db, sent, server, base, post };
}

test('page, code, verify, sign out', async () => {
  const { db, sent, server, base, post } = await start();
  try {
    const page = await (await fetch(base + '/signin')).text();
    assert.ok(page.includes('Continue with Google') === false && page.includes('Email me a code'));
    assert.equal((await post('/signin/code', { email: 'rick@example.com' }, { 'Sec-Fetch-Site': 'cross-site' })).status, 403);
    const badEmail = await post('/signin/code', { email: 'not an email' });
    assert.equal(badEmail.status, 400);
    assert.deepEqual(await badEmail.json(), { error: 'bad-email' });
    assert.deepEqual(await (await post('/signin/code', { email: 'rick@example.com' })).json(), { ok: true });
    await new Promise((r) => setImmediate(r));
    const wrong = sent[0].code === '000000' ? '111111' : '000000';
    const bad = await post('/signin/verify', { email: 'rick@example.com', code: wrong });
    assert.equal(bad.status, 400);
    assert.deepEqual(await bad.json(), { error: 'wrong', triesLeft: 4 });
    const good = await post('/signin/verify', { email: 'rick@example.com', code: sent[0].code, return: '/x' });
    assert.deepEqual(await good.json(), { next: '/x' });
    const cookie = good.headers.get('set-cookie') ?? '';
    const token = /cavecad_session=([^;]+)/.exec(cookie)![1];
    const who = findSession(db, token, Date.now());
    assert.equal(who?.email, 'rick@example.com');
    assert.equal(who?.name, 'rick');
    const out = await post('/signout', {}, { Cookie: `cavecad_session=${token}` });
    assert.match(out.headers.get('set-cookie') ?? '', /Max-Age=0/);
    assert.equal(findSession(db, token, Date.now()), null);
  } finally {
    server.close();
  }
});

test('the page offers Google only when it is configured', async () => {
  const db = openDb(':memory:');
  const d: SigninDeps = {
    db, publicUrl: 'https://cavecad.test', secure: true,
    codes: { db, secret: 'secret-0123456789abcdef', send: async () => true, mayGetIn: () => false },
    google: { clientId: 'cid', clientSecret: 'cs', redirectUri: 'https://cavecad.test/auth/google/callback', stateSecret: 'secret-0123456789abcdef' },
  };
  const server = createServer((req, res) => void handleSignin(d, req, res).then((done) => { if (!done) res.writeHead(404).end(); }));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  try {
    assert.ok((await (await fetch(base + '/signin')).text()).includes('Continue with Google'));
    const g = await fetch(base + '/auth/google?return=%2Fx', { redirect: 'manual' });
    assert.equal(g.status, 302);
    assert.match(g.headers.get('location') ?? '', /^https:\/\/accounts\.google\.com\//);
    assert.match(g.headers.get('set-cookie') ?? '', /cavecad_oauth=/);
    const cb = await fetch(base + '/auth/google/callback?state=x&code=y', { redirect: 'manual' });
    assert.equal(cb.status, 302);
    assert.equal(cb.headers.get('location'), '/signin?error=google');
    assert.match(cb.headers.get('set-cookie') ?? '', /cavecad_oauth=;.*Max-Age=0/);
  } finally {
    server.close();
  }
});

test('desktop hand-off', async () => {
  const { db, sent, server, post } = await start();
  try {
    const verifier = 'w'.repeat(43);
    const challenge = createHash('sha256').update(verifier).digest('base64url');
    await post('/signin/code', { email: 'rick@example.com' });
    await new Promise((r) => setImmediate(r));
    const r = await post('/signin/verify', { email: 'rick@example.com', code: sent[0].code, desktop: '1', challenge, state: 'st' });
    assert.equal(r.headers.get('set-cookie'), null);
    const next = new URL(((await r.json()) as { next: string }).next);
    assert.equal(next.protocol + '//' + next.host, 'cavecad://auth');
    assert.equal(next.searchParams.get('state'), 'st');
    assert.equal(redeemHandoff(db, next.searchParams.get('code')!, verifier, Date.now())?.email, 'rick@example.com');
  } finally {
    server.close();
  }
});

test('the desktop done page only offers a cavecad:// link', async () => {
  const { server, base } = await start();
  try {
    const evil = await (await fetch(base + '/signin?done=desktop&next=' + encodeURIComponent('javascript:alert(1)'))).text();
    assert.ok(!evil.includes('javascript:alert'));
    const good = await (await fetch(base + '/signin?done=desktop&next=' + encodeURIComponent('cavecad://auth?code=abc&state=st'))).text();
    assert.ok(good.includes("You're signed in"));
  } finally {
    server.close();
  }
});

test('the client address is the last X-Forwarded-For entry (the one the edge appended)', () => {
  const req = (xff: string | undefined) => ({ headers: xff === undefined ? {} : { 'x-forwarded-for': xff }, socket: { remoteAddress: '10.0.0.5' } }) as unknown as IncomingMessage;
  assert.equal(clientIp(req('1.1.1.1, 2.2.2.2')), '2.2.2.2');
  assert.equal(clientIp(req(' 3.3.3.3 ')), '3.3.3.3');
  assert.equal(clientIp(req(undefined)), '10.0.0.5');
  assert.equal(clientIp(req('')), '10.0.0.5');
});
