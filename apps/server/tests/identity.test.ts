import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync, sign, createPublicKey } from 'node:crypto';
import { makeWhoIs, type RequestLike } from '../src/identity.ts';

const EDGE = '192.168.86.13';
const req = (addr: string, headers: Record<string, string>): RequestLike => ({ socket: { remoteAddress: addr }, headers });
const gated = { 'x-cave-gate': 'tinyauth', 'remote-email': 'Rick@Example.com', 'remote-name': 'Rick', 'remote-sub': 'abc' };

test('TinyAuth headers from the edge on the gated site', async () => {
  const whoIs = makeWhoIs({ edgeAddress: EDGE });
  assert.deepEqual(await whoIs(req('::ffff:' + EDGE, gated)), { email: 'rick@example.com', name: 'Rick', provider: null, subject: 'abc' });
});

test('the same headers from anywhere else are ignored', async () => {
  const whoIs = makeWhoIs({ edgeAddress: EDGE });
  assert.equal(await whoIs(req('10.0.0.5', gated)), null);
  assert.equal(await whoIs(req(EDGE, { 'remote-email': 'rick@example.com' })), null, 'no gate header');
});

function jwt(key: ReturnType<typeof generateKeyPairSync>['privateKey'], alg: 'RS256' | 'EdDSA', claims: object) {
  const enc = (o: object) => Buffer.from(JSON.stringify(o)).toString('base64url');
  const head = enc({ alg, kid: 'k1', typ: 'JWT' }) + '.' + enc(claims);
  const sig = alg === 'RS256' ? sign('sha256', Buffer.from(head), key) : sign(null, Buffer.from(head), key);
  return head + '.' + sig.toString('base64url');
}

test('a TinyAuth ID token as a bearer', async () => {
  const { privateKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const jwk = { ...createPublicKey(privateKey).export({ format: 'jwk' }), kid: 'k1', alg: 'RS256' };
  const now = 1_800_000_000;
  const whoIs = makeWhoIs({ edgeAddress: EDGE, oidc: { issuer: 'https://auth.example', audience: 'cavecad-desktop', keys: async () => [jwk] }, nowSeconds: () => now });
  const good = { iss: 'https://auth.example', aud: 'cavecad-desktop', exp: now + 60, email: 'iris@example.com', name: 'Iris', sub: 'i1' };
  const auth = (t: string) => ({ authorization: 'Bearer ' + t });
  assert.deepEqual(await whoIs(req('1.2.3.4', auth(jwt(privateKey, 'RS256', good)))), { email: 'iris@example.com', name: 'Iris', provider: null, subject: 'i1' });
  assert.equal(await whoIs(req('1.2.3.4', auth(jwt(privateKey, 'RS256', { ...good, exp: now - 1 })))), null, 'expired');
  assert.equal(await whoIs(req('1.2.3.4', auth(jwt(privateKey, 'RS256', { ...good, aud: 'other' })))), null, 'audience');
  assert.equal(await whoIs(req('1.2.3.4', auth(jwt(privateKey, 'RS256', { ...good, iss: 'https://evil' })))), null, 'issuer');
  assert.equal(await whoIs(req('1.2.3.4', auth(jwt(privateKey, 'RS256', { ...good, email: undefined })))), null, 'no email');
  const stranger = generateKeyPairSync('rsa', { modulusLength: 2048 }).privateKey;
  assert.equal(await whoIs(req('1.2.3.4', auth(jwt(stranger, 'RS256', good)))), null, 'wrong key');
});

test('dev identity only on localhost', async () => {
  const whoIs = makeWhoIs({ edgeAddress: EDGE, devIdentity: 'nathan@example.com' });
  assert.equal((await whoIs(req('127.0.0.1', {})))?.email, 'nathan@example.com');
  assert.equal(await whoIs(req('192.168.1.9', {})), null);
  assert.equal(await makeWhoIs({ edgeAddress: EDGE })(req('127.0.0.1', {})), null);
});
