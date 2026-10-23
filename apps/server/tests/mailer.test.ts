import { test } from 'node:test';
import assert from 'node:assert/strict';
import { sendInvite } from '../src/mailer.ts';

const reply = (body: unknown) => (async () => ({ json: async () => body })) as unknown as typeof fetch;
const args = { to: 'rick@example.com', inviter: 'Nathan', link: 'https://cave.schonegg.stream/?signup=t' };

test('true only when the script says ok', async () => {
  assert.equal(await sendInvite({ url: 'https://s', key: 'K', fetch: reply({ ok: true }) }, args), true);
  assert.equal(await sendInvite({ url: 'https://s', key: 'K', fetch: reply({ ok: false, error: 'busy' }) }, args), false);
  assert.equal(await sendInvite({ url: 'https://s', key: 'K', fetch: (async () => { throw new Error('offline'); }) as typeof fetch }, args), false);
  assert.equal(await sendInvite({ url: '', key: '', fetch: reply({ ok: true }) }, args), false, 'no mailer set up');
});
