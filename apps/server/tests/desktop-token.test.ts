import { test } from 'node:test';
import assert from 'node:assert/strict';
import { makeDesktopToken } from '../src/desktop-token.ts';

test('exchanges a code with the token endpoint', async () => {
  const seen: string[] = [];
  const fake = (async (url: string, init?: RequestInit) => {
    if (url.endsWith('/.well-known/openid-configuration')) return { json: async () => ({ token_endpoint: 'https://auth.test/token' }) };
    seen.push(String(init?.body));
    return { ok: true, json: async () => ({ id_token: 'ID', refresh_token: 'R', expires_in: 3600, access_token: 'A' }) };
  }) as unknown as typeof fetch;
  const ex = makeDesktopToken({ issuer: 'https://auth.test', clientId: 'cavecad-desktop', clientSecret: 'S', fetch: fake });
  const out = await ex({ code: 'C', verifier: 'V' });
  assert.deepEqual(out, { status: 200, body: { id_token: 'ID', refresh_token: 'R', expires_in: 3600 } });
  const body = new URLSearchParams(seen[0]);
  assert.equal(body.get('grant_type'), 'authorization_code');
  assert.equal(body.get('code_verifier'), 'V');
  assert.equal(body.get('redirect_uri'), 'cavecad://auth');
  assert.equal(body.get('client_secret'), 'S');
  await ex({ refresh: 'R' });
  assert.equal(new URLSearchParams(seen[1]).get('grant_type'), 'refresh_token');
});

test('a refusal is sign-in-failed', async () => {
  const fake = (async (url: string) => (url.includes('well-known') ? { json: async () => ({ token_endpoint: 'https://auth.test/token' }) } : { ok: false, json: async () => ({ error: 'invalid_grant' }) })) as unknown as typeof fetch;
  const out = await makeDesktopToken({ issuer: 'https://auth.test', clientId: 'c', clientSecret: 's', fetch: fake })({ code: 'x', verifier: 'y' });
  assert.deepEqual(out, { status: 400, body: { error: 'sign-in-failed' } });
  assert.equal((await makeDesktopToken({ issuer: 'https://auth.test', clientId: 'c', clientSecret: 's', fetch: fake })({})).status, 400);
});
