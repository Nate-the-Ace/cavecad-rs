import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync } from 'node:crypto';
import { signPass, publicX, PASS_DAYS } from '../src/pass.ts';

test('a pass carries who and until when, and is two base64url parts', () => {
  const { privateKey } = generateKeyPairSync('ed25519');
  const p = signPass(privateKey, { memberId: 'm1', email: 'a@example.com', role: 'member' }, 1000);
  const [body, sig] = p.split('.');
  assert.ok(sig.length > 40);
  const claims = JSON.parse(Buffer.from(body, 'base64url').toString());
  assert.deepEqual(claims, { v: 1, memberId: 'm1', email: 'a@example.com', role: 'member', issued: 1000, expires: 1000 + PASS_DAYS * 86_400_000 });
  assert.match(publicX(privateKey), /^[A-Za-z0-9_-]{43}$/);
});
