import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { openDb } from '../src/db.ts';
import { createHandoff, redeemHandoff } from '../src/handoff.ts';

const who = { email: 'rick@example.com', name: 'Rick', provider: 'email' as const, subject: 'session' };
const verifier = 'v'.repeat(43);
const challenge = createHash('sha256').update(verifier).digest('base64url');
const now = 1_800_000_000_000;

test('redeems once with the right verifier', () => {
  const db = openDb(':memory:');
  const code = createHandoff(db, who, challenge, now);
  assert.equal(redeemHandoff(db, code, 'wrong', now), null);
  assert.deepEqual(redeemHandoff(db, code, verifier, now), who);
  assert.equal(redeemHandoff(db, code, verifier, now), null);
});
test('expires after a minute', () => {
  const db = openDb(':memory:');
  const code = createHandoff(db, who, challenge, now);
  assert.equal(redeemHandoff(db, code, verifier, now + 61_000), null);
});
