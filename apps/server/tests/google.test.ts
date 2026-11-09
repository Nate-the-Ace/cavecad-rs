import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync, sign } from 'node:crypto';
import { googleStart, googleFinish, safeReturn, validDesktop, type GoogleConfig } from '../src/google.ts';

const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
const jwk = { ...(publicKey.export({ format: 'jwk' }) as object), kid: 'g1', alg: 'RS256', use: 'sig' };
const now = 1_800_000_000_000;
const idToken = (claims: object) => {
  const h = Buffer.from(JSON.stringify({ alg: 'RS256', kid: 'g1' })).toString('base64url');
  const c = Buffer.from(JSON.stringify({ iss: 'https://accounts.google.com', aud: 'cid', exp: now / 1000 + 600, sub: 'g-1', email: 'Rick@gmail.com', email_verified: true, name: 'Rick', ...claims })).toString('base64url');
  return `${h}.${c}.${sign('sha256', Buffer.from(h + '.' + c), privateKey).toString('base64url')}`;
};
const cfg = (token: string): GoogleConfig => ({
  clientId: 'cid', clientSecret: 'secret', redirectUri: 'https://cavecad.test/auth/google/callback', stateSecret: 'state-secret-0123456789',
  keys: async () => [jwk],
  fetch: (async () => ({ ok: true, json: async () => ({ id_token: token }) })) as unknown as typeof fetch,
});
const cookieOf = (setCookie: string) => setCookie.split(';')[0];

test('start then finish', async () => {
  const c = cfg(idToken({}));
  const s = googleStart(c, { returnTo: '/cave/1', desktop: null }, now);
  const u = new URL(s.location);
  assert.equal(u.origin + u.pathname, 'https://accounts.google.com/o/oauth2/v2/auth');
  assert.equal(u.searchParams.get('client_id'), 'cid');
  assert.equal(u.searchParams.get('code_challenge_method'), 'S256');
  assert.match(s.cookie, /^cavecad_oauth=.+; HttpOnly; Secure; SameSite=Lax; Path=\/auth\/google; Max-Age=600$/);
  const q = new URLSearchParams({ code: 'c1', state: u.searchParams.get('state')! });
  const out = await googleFinish(c, q, cookieOf(s.cookie), now);
  assert.deepEqual(out, { ok: true, who: { email: 'rick@gmail.com', name: 'Rick', provider: 'google', subject: 'g-1' }, returnTo: '/cave/1', desktop: null });
});

test('failures', async () => {
  const c = cfg(idToken({}));
  const s = googleStart(c, { returnTo: '/', desktop: null }, now);
  const state = new URL(s.location).searchParams.get('state')!;
  assert.deepEqual(await googleFinish(c, new URLSearchParams({ code: 'c', state: 'other' }), cookieOf(s.cookie), now), { ok: false, error: 'failed' });
  assert.deepEqual(await googleFinish(c, new URLSearchParams({ code: 'c', state }), '', now), { ok: false, error: 'failed' });
  assert.deepEqual(await googleFinish(c, new URLSearchParams({ code: 'c', state }), cookieOf(s.cookie) + 'x', now), { ok: false, error: 'failed' });
  assert.deepEqual(await googleFinish(c, new URLSearchParams({ error: 'access_denied', state }), cookieOf(s.cookie), now), { ok: false, error: 'cancelled' });
  assert.deepEqual(await googleFinish(c, new URLSearchParams({ code: 'c', state }), cookieOf(s.cookie), now + 700_000), { ok: false, error: 'failed' }, 'state cookie expired');
  const unverified = cfg(idToken({ email_verified: false }));
  const s2 = googleStart(unverified, { returnTo: '/', desktop: null }, now);
  assert.deepEqual(await googleFinish(unverified, new URLSearchParams({ code: 'c', state: new URL(s2.location).searchParams.get('state')! }), cookieOf(s2.cookie), now), { ok: false, error: 'unverified' });
  const bad = cfg(idToken({ aud: 'someone-else' }));
  const s3 = googleStart(bad, { returnTo: '/', desktop: null }, now);
  assert.deepEqual(await googleFinish(bad, new URLSearchParams({ code: 'c', state: new URL(s3.location).searchParams.get('state')! }), cookieOf(s3.cookie), now), { ok: false, error: 'failed' });
});

test('safeReturn', () => {
  assert.equal(safeReturn('/€'), '/%E2%82%AC');
  assert.equal(safeReturn('/cave/1?x=2'), '/cave/1?x=2');
  assert.equal(safeReturn('//evil.com'), '/');
  assert.equal(safeReturn('https://evil.com'), '/');
  assert.equal(safeReturn('/\\evil.com'), '/');
  assert.equal(safeReturn(undefined), '/');
  assert.equal(safeReturn('/\t/evil.com'), '/');
  assert.equal(safeReturn('/\n/evil.com'), '/');
  assert.equal(safeReturn('/%09/evil.com'), '/%09/evil.com');
});

const CH = 'A'.repeat(43);
test('validDesktop', () => {
  assert.deepEqual(validDesktop({ desktop: '1', challenge: CH, state: 'st_1-x' }), { challenge: CH, state: 'st_1-x' });
  assert.equal(validDesktop({ desktop: '0', challenge: CH, state: 's' }), null);
  assert.equal(validDesktop({ desktop: '1', challenge: 'short', state: 's' }), null);
  assert.equal(validDesktop({ desktop: '1', challenge: CH, state: '' }), null);
  assert.equal(validDesktop({ desktop: '1', challenge: CH, state: 'a b' }), null);
  assert.equal(validDesktop({ desktop: '1', challenge: CH, state: 'a'.repeat(129) }), null);
  assert.equal(validDesktop({ desktop: '1' }), null);
});

test('googleStart drops invalid desktop params, keeps valid', async () => {
  const c = cfg(idToken({}));
  for (const [d, want] of [[{ challenge: 'bad', state: 's' }, null], [{ challenge: CH, state: 's' }, { challenge: CH, state: 's' }]] as const) {
    const s = googleStart(c, { returnTo: '/', desktop: d }, now);
    const out = await googleFinish(c, new URLSearchParams({ code: 'c', state: new URL(s.location).searchParams.get('state')! }), cookieOf(s.cookie), now);
    assert.ok(out.ok);
    assert.deepEqual(out.ok && out.desktop, want);
  }
});

test('missing sub, token endpoint refusal, missing code all fail', async () => {
  const noSub = cfg(idToken({ sub: undefined }));
  const s = googleStart(noSub, { returnTo: '/', desktop: null }, now);
  const st = new URL(s.location).searchParams.get('state')!;
  assert.deepEqual(await googleFinish(noSub, new URLSearchParams({ code: 'c', state: st }), cookieOf(s.cookie), now), { ok: false, error: 'failed' });
  const refused = { ...cfg(idToken({})), fetch: (async () => ({ ok: false, json: async () => ({ id_token: idToken({}) }) })) as unknown as typeof fetch };
  const s2 = googleStart(refused, { returnTo: '/', desktop: null }, now);
  assert.deepEqual(await googleFinish(refused, new URLSearchParams({ code: 'c', state: new URL(s2.location).searchParams.get('state')! }), cookieOf(s2.cookie), now), { ok: false, error: 'failed' });
  const c = cfg(idToken({}));
  const s3 = googleStart(c, { returnTo: '/', desktop: null }, now);
  assert.deepEqual(await googleFinish(c, new URLSearchParams({ state: new URL(s3.location).searchParams.get('state')! }), cookieOf(s3.cookie), now), { ok: false, error: 'failed' });
});
