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

test('mail failure still answers ok, even a throwing or rejecting send', async () => {
  const { d } = setup(false);
  assert.deepEqual(await requestCode(d, 'rick@example.com', '1.1.1.1', now), { ok: true });
  const thrower = { ...d, send: (() => { throw new Error('boom'); }) as unknown as typeof d.send };
  assert.deepEqual(await requestCode(thrower, 'rick@example.com', '1.1.1.1', now + 61_000), { ok: true });
  const rejecter = { ...d, send: async () => { throw new Error('boom'); } };
  assert.deepEqual(await requestCode(rejecter, 'rick@example.com', '1.1.1.1', now + 122_000), { ok: true });
  await new Promise((r) => setImmediate(r));
});

test('verify cannot tell a member from a stranger', async () => {
  const { d } = setup();
  await requestCode(d, 'rick@example.com', '1.1.1.1', now);
  await requestCode(d, 'stranger@example.com', '1.1.1.1', now);
  for (const left of [4, 3]) {
    const a = verifyCode(d, 'rick@example.com', '000000', now);
    const b = verifyCode(d, 'stranger@example.com', '000000', now);
    if (a.ok === false && a.error === 'wrong') assert.deepEqual(b, a);
    else assert.fail('member answered ' + JSON.stringify(a));
    assert.equal((b as { triesLeft: number }).triesLeft, left);
  }
});

test('thirty wrong tries a day lock an email out, member or stranger', async () => {
  const { d, db, sent } = setup();
  for (const who of ['rick@example.com', 'stranger@example.com']) {
    await requestCode(d, who, '1.1.1.1', now);
    // wrong tries are counted per email across codes: six codes of five tries
    for (let round = 0; round < 6; round++) {
      const t = now + round * 15 * 60_000;
      if (round) await requestCode(d, who, '1.1.1.1', t);
      for (let i = 0; i < 5; i++) verifyCode(d, who, '000000', t);
    }
    const t = now + 6 * 15 * 60_000;
    await requestCode(d, who, '1.1.1.1', t);
    const right = who === 'rick@example.com' ? sent.at(-1)!.code : '000000';
    assert.deepEqual(verifyCode(d, who, right, t), { ok: false, error: 'expired' }, who + ' locked out, even with the right code');
    assert.equal((db.prepare('SELECT COUNT(*) AS n FROM code_failures WHERE email = ?').get(who) as { n: number }).n, 30);
    // a day on, the failures age out and the member can sign in again
    const later = t + 86_400_001;
    await requestCode(d, who, '1.1.1.1', later);
    if (who === 'rick@example.com') assert.deepEqual(verifyCode(d, who, sent.at(-1)!.code, later), { ok: true, email: who });
    else assert.deepEqual(verifyCode(d, who, '000000', later), { ok: false, error: 'wrong', triesLeft: 4 });
  }
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
