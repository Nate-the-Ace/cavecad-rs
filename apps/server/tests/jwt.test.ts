import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync, sign } from 'node:crypto';
import { verifyJwt, jwksKeys } from '../src/jwt.ts';

const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
const jwk = { ...(publicKey.export({ format: 'jwk' }) as object), kid: 'k1', alg: 'RS256', use: 'sig' };
const now = 1_800_000_000;
const make = (claims: object, head: object = { alg: 'RS256', kid: 'k1' }) => {
  const h = Buffer.from(JSON.stringify(head)).toString('base64url');
  const c = Buffer.from(JSON.stringify(claims)).toString('base64url');
  return `${h}.${c}.${sign('sha256', Buffer.from(h + '.' + c), privateKey).toString('base64url')}`;
};
const good = { iss: 'https://accounts.google.com', aud: 'client', exp: now + 600, sub: '1', email: 'a@b.com' };
const opts = { keys: async () => [jwk], issuers: ['https://accounts.google.com', 'accounts.google.com'], audience: 'client', now };

test('a good token gives its claims', async () => {
  assert.equal((await verifyJwt(make(good), opts))?.email, 'a@b.com');
});
test('bad tokens give null', async () => {
  assert.equal(await verifyJwt(make({ ...good, iss: 'x' }), opts), null);
  assert.equal(await verifyJwt(make({ ...good, aud: 'other' }), opts), null);
  assert.equal(await verifyJwt(make({ ...good, exp: now - 120 }), opts), null);
  assert.equal(await verifyJwt(make(good, { alg: 'none', kid: 'k1' }), opts), null);
  const t = make(good);
  assert.equal(await verifyJwt(t.slice(0, -4) + 'AAAA', opts), null);
  assert.equal(await verifyJwt('junk', opts), null);
});
test('jwksKeys caches and refetches for an unknown kid at most once a minute', async () => {
  let calls = 0;
  const fake = (async () => { calls++; return { ok: true, json: async () => ({ keys: [jwk] }) }; }) as unknown as typeof fetch;
  const get = jwksKeys('https://example/jwks', fake);
  await get('k1'); await get('k1');
  assert.equal(calls, 1);
  await get('unknown'); await get('unknown');
  assert.equal(calls, 2);
});

test('now in milliseconds is a programming error', async () => {
  await assert.rejects(verifyJwt(make(good), { ...opts, now: now * 1000 }), /seconds/);
});
test('a null entry in the key set does not throw', async () => {
  const keys = async () => [null, 'x', jwk] as unknown as (typeof jwk)[];
  assert.equal((await verifyJwt(make(good), { ...opts, keys }))?.email, 'a@b.com');
});
