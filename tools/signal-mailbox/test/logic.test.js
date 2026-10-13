// Run with: node --test tools/signal-mailbox/test
const test = require('node:test');
const assert = require('node:assert');
const L = require('../Logic.gs');

/** A stand-in for Apps Script's CacheService: values expire by the clock the test sets. */
function fakeCache() {
  const store = new Map();
  let now = 0;
  const live = (k) => store.has(k) && store.get(k).until > now;
  return {
    tick: (s) => (now += s),
    get: (k) => (live(k) ? store.get(k).v : null),
    put: (k, v, s) => void store.set(k, { v, until: now + s }),
    getAll: (keys) => Object.fromEntries(keys.filter(live).map((k) => [k, store.get(k).v])),
    remove: (k) => void store.delete(k),
    size: () => [...store.keys()].filter(live).length,
  };
}
const setup = () => {
  const cache = fakeCache();
  let date = new Date('2026-10-13T14:00:00Z');
  const env = () => ({ cache, expected: 'KEY1', now: date, locked: (f) => f() });
  const call = (p) => L.run(env(), { key: 'KEY1', session: 'class12345', ...p });
  return { cache, call, env, setDate: (d) => (date = new Date(d)) };
};

test('only the three kinds of note, in a valid session, with the key', () => {
  const { call, env } = setup();
  assert.deepStrictEqual(L.run(env(), { op: 'list', key: 'nope', session: 'class12345' }), { ok: false, error: 'key' });
  assert.deepStrictEqual(call({ op: 'list', session: 'short' }), { ok: false, error: 'session' });
  assert.deepStrictEqual(call({ op: 'put', name: 'junk:abcd', body: 'x' }), { ok: false, error: 'name' });
  assert.deepStrictEqual(call({ op: 'put', name: 'hello:../..', body: 'x' }), { ok: false, error: 'name' });
  assert.deepStrictEqual(call({ op: 'put', name: 'hello:abcd', body: 'x'.repeat(L.MAX_VALUE + 1) }), { ok: false, error: 'too-big' });
  assert.deepStrictEqual(call({ op: 'put', name: 'hello:abcd', body: '' }), { ok: false, error: 'empty' });
  assert.deepStrictEqual(call({ op: 'frobnicate' }), { ok: false, error: 'op' });
});

test('a student says hello, the teacher answers with an offer, the student replies', () => {
  const { call } = setup();
  call({ op: 'put', name: 'hello:s1abc', body: '1' });
  call({ op: 'put', name: 'hello:s2abc', body: '1' });
  assert.deepStrictEqual(call({ op: 'list', prefix: 'hello:' }).items.map((i) => i.n), ['hello:s1abc', 'hello:s2abc']);
  call({ op: 'put', name: 'offer:s1abc', body: 'CAVE1-offer-for-1' });
  assert.strictEqual(call({ op: 'get', name: 'offer:s1abc' }).value, 'CAVE1-offer-for-1');
  assert.strictEqual(call({ op: 'get', name: 'offer:s2abc' }).value, null, 'nothing for the other student yet');
  call({ op: 'put', name: 'answer:s1abc', body: 'CAVE1-answer-1' });
  assert.deepStrictEqual(call({ op: 'list', prefix: 'answer:' }).items, [{ n: 'answer:s1abc', v: 'CAVE1-answer-1' }]);
  call({ op: 'del', name: 'hello:s1abc' });
  assert.deepStrictEqual(call({ op: 'list', prefix: 'hello:' }).items.map((i) => i.n), ['hello:s2abc'], 'a note taken away is gone from the list');
});

test('sessions are separate, and notes expire on their own', () => {
  const { cache, call } = setup();
  call({ op: 'put', name: 'hello:s1abc', body: '1' });
  assert.strictEqual(call({ op: 'list', session: 'other12345', prefix: 'hello:' }).items.length, 0);
  cache.tick(L.TTL + 1);
  assert.strictEqual(call({ op: 'get', name: 'hello:s1abc' }).value, null);
  assert.deepStrictEqual(call({ op: 'list', prefix: 'hello:' }).items, [], 'an expired note drops out of the list');
});

test('a busy minute is refused, and the next minute is fresh', () => {
  const { call, setDate } = setup();
  for (let i = 0; i < L.MAX_PER_MINUTE; i++) assert.strictEqual(call({ op: 'list', prefix: 'hello:' }).ok, true);
  assert.deepStrictEqual(call({ op: 'list', prefix: 'hello:' }), { ok: false, error: 'busy' });
  setDate('2026-10-13T14:01:05Z');
  assert.strictEqual(call({ op: 'list', prefix: 'hello:' }).ok, true);
});

test('a session holds only so many notes, the newest kept', () => {
  const { call } = setup();
  for (let i = 0; i < L.MAX_NAMES + 5; i++) call({ op: 'put', name: 'hello:s' + String(i).padStart(4, '0'), body: '1' });
  const names = call({ op: 'list', prefix: 'hello:' }).items.map((i) => i.n);
  assert.strictEqual(names.length, L.MAX_NAMES);
  assert.strictEqual(names.at(-1), 'hello:s' + String(L.MAX_NAMES + 4).padStart(4, '0'));
});

test('a damaged index is read as empty, not an error', () => {
  assert.deepStrictEqual(L.parseIndex('not json'), []);
  assert.deepStrictEqual(L.parseIndex('{"a":1}'), []);
  assert.deepStrictEqual(L.parseIndex('["hello:abcd","evil","answer:x/../y"]'), ['hello:abcd']);
});
