import { test } from 'node:test';
import assert from 'node:assert/strict';
import { openDb } from '../src/db.ts';
import { requestCode, verifyCode, CODE_MS } from '../src/email-code.ts';

const now = 1_800_000_000_000;
function setup(sendOk = true) {
  const db = openDb(':memory:');
  const sent: { to: string; code: string }[] = [];
  const d = { db, secret: 'test-secret-0123456789abcdef', send: async (to: string, code: string) => (sent.push({ to, code }), sendOk), mayGetIn: (e: string) => e === 'rick@example.com' };
  return { db, d, sent };
}

test('an eligible email gets a code; a stranger gets the same answer and nothing', async () => {
  const { d, sent, db } = setup();
  assert.deepEqual(await requestCode(d, ' Rick@Example.com ', '1.1.1.1', now), { ok: true });
  assert.equal(sent.length, 1);
  assert.match(sent[0].code, /^\d{6}$/);
  assert.equal(sent[0].to, 'rick@example.com');
  const stored = db.prepare('SELECT code_hmac FROM email_codes').get() as { code_hmac: string };
  assert.notEqual(stored.code_hmac, sent[0].code);
  assert.deepEqual(await requestCode(d, 'stranger@example.com', '1.1.1.1', now), { ok: true });
  assert.equal(sent.length, 1);
  assert.deepEqual(await requestCode(d, 'not an email', '1.1.1.1', now), { ok: false, error: 'bad-email' });
});

test('limits', async () => {
  const { d, sent } = setup();
  await requestCode(d, 'rick@example.com', '1.1.1.1', now);
  await requestCode(d, 'rick@example.com', '1.1.1.1', now + 30_000);
  assert.equal(sent.length, 1, 'one a minute');
  for (let i = 1; i <= 5; i++) await requestCode(d, 'rick@example.com', '1.1.1.1', now + i * 61_000);
  assert.equal(sent.length, 5, 'five an hour');
  const { d: d2, sent: s2 } = setup();
  for (let i = 0; i < 21; i++) await requestCode({ ...d2, mayGetIn: () => true }, `p${i}@example.com`, '9.9.9.9', now + i);
  assert.equal(s2.length, 20, 'twenty an hour from one address');
});

test('mail failure is reported', async () => {
  const { d } = setup(false);
  assert.deepEqual(await requestCode(d, 'rick@example.com', '1.1.1.1', now), { ok: false, error: 'send-failed' });
});

test('verifying', async () => {
  const { d, sent } = setup();
  await requestCode(d, 'rick@example.com', '1.1.1.1', now);
  const code = sent[0].code;
  const wrong = code === '000000' ? '111111' : '000000';
  assert.deepEqual(verifyCode(d, 'rick@example.com', wrong, now), { ok: false, error: 'wrong', triesLeft: 4 });
  assert.deepEqual(verifyCode(d, 'RICK@example.com', code, now), { ok: true, email: 'rick@example.com' });
  assert.deepEqual(verifyCode(d, 'rick@example.com', code, now), { ok: false, error: 'expired' }, 'works once');
  assert.deepEqual(verifyCode(d, 'nobody@example.com', code, now), { ok: false, error: 'expired' });
});

test('tries run out, and codes expire', async () => {
  const { d, sent } = setup();
  await requestCode(d, 'rick@example.com', '1.1.1.1', now);
  const code = sent[0].code;
  const wrong = code === '000000' ? '111111' : '000000';
  for (const left of [4, 3, 2, 1]) assert.deepEqual(verifyCode(d, 'rick@example.com', wrong, now), { ok: false, error: 'wrong', triesLeft: left });
  assert.deepEqual(verifyCode(d, 'rick@example.com', wrong, now), { ok: false, error: 'expired' });
  assert.deepEqual(verifyCode(d, 'rick@example.com', code, now), { ok: false, error: 'expired' }, 'dead after five wrong');
  await requestCode(d, 'rick@example.com', '1.1.1.1', now + 61_000);
  assert.deepEqual(verifyCode(d, 'rick@example.com', sent[1].code, now + 61_000 + CODE_MS + 1), { ok: false, error: 'expired' });
});
