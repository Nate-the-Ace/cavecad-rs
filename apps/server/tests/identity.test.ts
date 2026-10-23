import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync, sign, createPublicKey, type KeyObject } from 'node:crypto';
import { makeWhoIs, discoveredKeys, type RequestLike } from '../src/identity.ts';

const EDGE = '192.168.86.13';
const SECRET = 'a-long-random-gate-secret-0123456789';
const req = (addr: string | undefined, headers: Record<string, string>): RequestLike => ({ socket: { remoteAddress: addr }, headers });
const gated = { 'x-cave-gate': SECRET, 'remote-email': 'Rick@Example.com', 'remote-name': 'Rick', 'remote-sub': 'abc' };

test('TinyAuth headers from the edge on the gated site', async () => {
  const whoIs = makeWhoIs({ edgeAddress: EDGE, gateSecret: SECRET });
  assert.deepEqual(await whoIs(req('::ffff:' + EDGE, gated)), { email: 'rick@example.com', name: 'Rick', provider: null, subject: 'abc' });
});

test('a name TinyAuth sends as UTF-8 reads right (Node hands header bytes over as latin1)', async () => {
  const whoIs = makeWhoIs({ edgeAddress: EDGE, gateSecret: SECRET });
  const asNodeSeesIt = Buffer.from('José Núñez', 'utf8').toString('latin1');
  assert.equal((await whoIs(req(EDGE, { ...gated, 'remote-name': asNodeSeesIt })))?.name, 'José Núñez');
  assert.equal((await whoIs(req(EDGE, { ...gated, 'remote-name': 'Ren\xe9' })))?.name, 'Ren\xe9', 'bytes that are not UTF-8 are kept as they came');
});

test('the same headers from anywhere else are ignored', async () => {
  const whoIs = makeWhoIs({ edgeAddress: EDGE, gateSecret: SECRET });
  assert.equal(await whoIs(req('10.0.0.5', gated)), null);
  assert.equal(await whoIs(req(EDGE, { 'remote-email': 'rick@example.com' })), null, 'no gate header');
});

test('the gate secret must match exactly and must be set', async () => {
  const whoIs = makeWhoIs({ edgeAddress: EDGE, gateSecret: SECRET });
  assert.equal(await whoIs(req(EDGE, { ...gated, 'x-cave-gate': 'tinyauth' })), null, 'wrong gate value');
  assert.equal(await whoIs(req(EDGE, { ...gated, 'x-cave-gate': SECRET.slice(0, -1) + 'X' })), null, 'same length, wrong value');
  assert.equal(await makeWhoIs({ edgeAddress: '', gateSecret: SECRET })(req(undefined, gated)), null, 'empty edge address, no remote address');
  assert.equal(await makeWhoIs({ edgeAddress: EDGE, gateSecret: '' })(req(EDGE, { ...gated, 'x-cave-gate': '' })), null, 'empty gate secret');
});

function jwt(key: KeyObject, alg: 'RS256' | 'EdDSA', claims: object, head: object = { alg, kid: 'k1', typ: 'JWT' }) {
  const enc = (o: object) => Buffer.from(JSON.stringify(o)).toString('base64url');
  const body = enc(head) + '.' + enc(claims);
  const sig = alg === 'RS256' ? sign('sha256', Buffer.from(body), key) : sign(null, Buffer.from(body), key);
  return body + '.' + sig.toString('base64url');
}

const now = 1_800_000_000;
const good = { iss: 'https://auth.example', aud: 'cavecad-desktop', exp: now + 60, email: 'iris@example.com', name: 'Iris', sub: 'i1' };
const auth = (t: string) => ({ authorization: 'Bearer ' + t });
const bearerWhoIs = (keys: object[]) => makeWhoIs({ edgeAddress: EDGE, gateSecret: SECRET, oidc: { issuer: 'https://auth.example', audience: 'cavecad-desktop', keys: async () => keys }, nowSeconds: () => now });
const rsa = generateKeyPairSync('rsa', { modulusLength: 2048 }).privateKey;
const rsaJwk = { ...createPublicKey(rsa).export({ format: 'jwk' }), kid: 'k1', alg: 'RS256' };

test('a TinyAuth ID token as a bearer', async () => {
  const whoIs = bearerWhoIs([rsaJwk]);
  assert.deepEqual(await whoIs(req('1.2.3.4', auth(jwt(rsa, 'RS256', good)))), { email: 'iris@example.com', name: 'Iris', provider: null, subject: 'i1' });
  assert.equal(await whoIs(req('1.2.3.4', auth(jwt(rsa, 'RS256', { ...good, exp: now - 61 })))), null, 'expired past the leeway');
  assert.ok(await whoIs(req('1.2.3.4', auth(jwt(rsa, 'RS256', { ...good, exp: now - 30 })))), 'expired within the leeway');
  assert.equal(await whoIs(req('1.2.3.4', auth(jwt(rsa, 'RS256', { ...good, nbf: now + 120 })))), null, 'not yet valid');
  assert.ok(await whoIs(req('1.2.3.4', auth(jwt(rsa, 'RS256', { ...good, nbf: now + 30 })))), 'nbf within the leeway');
  assert.equal(await whoIs(req('1.2.3.4', auth(jwt(rsa, 'RS256', { ...good, aud: 'other' })))), null, 'audience');
  assert.equal(await whoIs(req('1.2.3.4', auth(jwt(rsa, 'RS256', { ...good, aud: ['cavecad-desktop', 'other'] })))), null, 'several audiences, no azp');
  assert.ok(await whoIs(req('1.2.3.4', auth(jwt(rsa, 'RS256', { ...good, aud: ['cavecad-desktop', 'other'], azp: 'cavecad-desktop' })))), 'several audiences, azp is us');
  assert.equal(await whoIs(req('1.2.3.4', auth(jwt(rsa, 'RS256', { ...good, iss: 'https://evil' })))), null, 'issuer');
  assert.equal(await whoIs(req('1.2.3.4', auth(jwt(rsa, 'RS256', { ...good, email: undefined })))), null, 'no email');
  const stranger = generateKeyPairSync('rsa', { modulusLength: 2048 }).privateKey;
  assert.equal(await whoIs(req('1.2.3.4', auth(jwt(stranger, 'RS256', good)))), null, 'wrong key');
});

test('email_verified, when present, must be true', async () => {
  const whoIs = bearerWhoIs([rsaJwk]);
  assert.equal(await whoIs(req('1.2.3.4', auth(jwt(rsa, 'RS256', { ...good, email_verified: false })))), null, 'false');
  assert.equal(await whoIs(req('1.2.3.4', auth(jwt(rsa, 'RS256', { ...good, email_verified: 'true' })))), null, 'a string');
  assert.ok(await whoIs(req('1.2.3.4', auth(jwt(rsa, 'RS256', { ...good, email_verified: true })))), 'true');
});

test('a bearer whose parts are not objects is refused, not thrown', async () => {
  const whoIs = bearerWhoIs([rsaJwk]);
  assert.equal(await whoIs(req('1.2.3.4', auth('bnVsbA.e30.AAAA'))), null, 'null header');
  assert.equal(await whoIs(req('1.2.3.4', auth('e30.bnVsbA.AAAA'))), null, 'null claims');
  assert.equal(await whoIs(req('1.2.3.4', auth('W10.e30.AAAA'))), null, 'array header');
});

test('an EdDSA bearer with an Ed25519 key', async () => {
  const ed = generateKeyPairSync('ed25519').privateKey;
  const edJwk = { ...createPublicKey(ed).export({ format: 'jwk' }), kid: 'k1' };
  const whoIs = bearerWhoIs([edJwk]);
  assert.equal((await whoIs(req('1.2.3.4', auth(jwt(ed, 'EdDSA', good)))))?.email, 'iris@example.com');
  assert.equal(await bearerWhoIs([{ ...edJwk, use: 'enc' }])(req('1.2.3.4', auth(jwt(ed, 'EdDSA', good)))), null, 'an encryption key signs nothing');
});

test('the alg must match the key type', async () => {
  // An EdDSA header naming the RSA key is refused even though the RSA key exists.
  const whoIs = bearerWhoIs([rsaJwk]);
  const ed = generateKeyPairSync('ed25519').privateKey;
  assert.equal(await whoIs(req('1.2.3.4', auth(jwt(ed, 'EdDSA', good)))), null);
  assert.equal(await whoIs(req('1.2.3.4', auth(jwt(rsa, 'RS256', good, { alg: 'EdDSA', kid: 'k1' })))), null);
});

test('without a kid every matching key is tried', async () => {
  const other = generateKeyPairSync('rsa', { modulusLength: 2048 }).privateKey;
  const otherJwk = { ...createPublicKey(other).export({ format: 'jwk' }), kid: 'k0' };
  const whoIs = bearerWhoIs([otherJwk, rsaJwk]);
  assert.ok(await whoIs(req('1.2.3.4', auth(jwt(rsa, 'RS256', good, { alg: 'RS256' })))));
});

test('discovered keys: cache, failure back-off, one refetch for an unknown kid', async () => {
  let calls = 0;
  let fail = false;
  let keys: object[] = [{ kty: 'RSA', kid: 'a' }];
  const fake = (async (url: string) => {
    calls++;
    if (fail) return { ok: false, status: 500, json: async () => ({}) };
    if (url.endsWith('/.well-known/openid-configuration')) return { ok: true, json: async () => ({ jwks_uri: 'https://auth.example/jwks' }) };
    return { ok: true, json: async () => ({ keys }) };
  }) as unknown as typeof fetch;
  const realNow = Date.now;
  let t = 1_000_000;
  Date.now = () => t;
  try {
    const get = discoveredKeys('https://auth.example/', fake);
    assert.deepEqual(await get('a'), keys);
    assert.equal(calls, 2);
    await get('a');
    assert.equal(calls, 2, 'cached');
    keys = [{ kty: 'RSA', kid: 'b' }];
    assert.deepEqual(await get('b'), keys, 'unknown kid refetches');
    assert.equal(calls, 4);
    await get('c');
    assert.equal(calls, 4, 'no second refetch within a minute');
    t += 3_600_001;
    fail = true;
    assert.deepEqual(await get('b'), [], 'failure gives no keys');
    const after = calls;
    t += 10_000;
    assert.deepEqual(await get('b'), []);
    assert.equal(calls, after, 'failure cached for 30 s');
    fail = false;
    t += 30_000;
    assert.deepEqual(await get('b'), keys, 'retried after 30 s');
  } finally {
    Date.now = realNow;
  }
});

test('dev identity only on localhost', async () => {
  const whoIs = makeWhoIs({ edgeAddress: EDGE, gateSecret: SECRET, devIdentity: 'nathan@example.com' });
  assert.equal((await whoIs(req('127.0.0.1', {})))?.email, 'nathan@example.com');
  assert.equal(await whoIs(req('192.168.1.9', {})), null);
  assert.equal(await makeWhoIs({ edgeAddress: EDGE, gateSecret: SECRET })(req('127.0.0.1', {})), null);
});
