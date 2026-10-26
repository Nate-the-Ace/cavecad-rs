import { test } from 'node:test';
import assert from 'node:assert/strict';
import { broadcast, listen, listeners, writeOrDrop, MAX_BUFFERED, MAX_PER_MEMBER, type Out } from '../src/streams.ts';

/** A stand-in response: records what is written, and how full its buffer claims to be. */
function fake(buffered = 0) {
  const o = {
    out: [] as string[],
    writableLength: buffered,
    destroyed: false,
    write(t: string) {
      o.out.push(t);
      return true;
    },
    destroy() {
      o.destroyed = true;
      return o;
    },
  };
  return o;
}
const asOut = (f: ReturnType<typeof fake>) => f as unknown as Out;
const numbers = (f: ReturnType<typeof fake>) => f.out.map((t) => JSON.parse(t)).map((l) => (l.t === 'entry' ? l.number : l.t));

test('a slow client is dropped once its unsent bytes pass the limit', () => {
  const ok = fake(1024);
  writeOrDrop(asOut(ok), 'x\n');
  assert.equal(ok.destroyed, false);
  const slow = fake(MAX_BUFFERED + 1);
  writeOrDrop(asOut(slow), 'x\n');
  assert.equal(slow.destroyed, true);
  writeOrDrop(asOut(slow), 'y\n');
  assert.deepEqual(slow.out, ['x\n'], 'nothing more is written to a dropped client');
});

test('broadcast drops a slow live stream and keeps a fast one', () => {
  const fast = fake();
  const slow = fake(MAX_BUFFERED + 1);
  listen('cave-slow', 'm1', asOut(fast)).goLive(0);
  listen('cave-slow', 'm1', asOut(slow)).goLive(0);
  broadcast('cave-slow', { t: 'entry', number: 1 });
  assert.equal(slow.destroyed, true);
  assert.equal(fast.destroyed, false);
  assert.deepEqual(numbers(fast), ['live', 1]);
});

test('lines arriving during catch-up wait, then go out once each, after live', () => {
  const f = fake();
  const l = listen('cave-queue', 'm1', asOut(f));
  broadcast('cave-queue', { t: 'entry', number: 3 });
  broadcast('cave-queue', { t: 'entry', number: 4 });
  broadcast('cave-queue', { t: 'entry', number: 5 });
  assert.deepEqual(f.out, [], 'nothing is written before live');
  l.goLive(4); // catch-up already sent up to 4
  broadcast('cave-queue', { t: 'entry', number: 5 }); // a repeat is never sent twice
  broadcast('cave-queue', { t: 'entry', number: 6 });
  assert.deepEqual(numbers(f), ['live', 5, 6]);
  l.stop();
});

test('two streams on one cave: closing one (twice) leaves the other listening', () => {
  const a = fake();
  const b = fake();
  const la = listen('cave-two', 'm1', asOut(a));
  const lb = listen('cave-two', 'm1', asOut(b));
  la.goLive(0);
  lb.goLive(0);
  la.stop();
  la.stop();
  assert.equal(listeners('cave-two'), 1);
  broadcast('cave-two', { t: 'entry', number: 1 });
  assert.deepEqual(numbers(b), ['live', 1]);
  assert.deepEqual(numbers(a), ['live']);
  lb.stop();
  assert.equal(listeners('cave-two'), 0);
});

test('a late second stop never removes a newer stream on the same cave', () => {
  const a = fake();
  const b = fake();
  const la = listen('cave-late', 'm1', asOut(a));
  la.stop(); // the cave's set is now gone
  const lb = listen('cave-late', 'm1', asOut(b));
  lb.goLive(0);
  la.stop(); // late, from a second close event
  assert.equal(listeners('cave-late'), 1);
  broadcast('cave-late', { t: 'entry', number: 1 });
  assert.deepEqual(numbers(b), ['live', 1]);
  lb.stop();
});

test(`a member holds at most ${MAX_PER_MEMBER} streams; the oldest closes when another opens`, () => {
  const all = Array.from({ length: MAX_PER_MEMBER + 1 }, () => fake());
  const ls = all.map((f, i) => listen(`cave-cap-${i % 3}`, 'capped', asOut(f)));
  assert.equal(all[0].destroyed, true);
  assert.equal(all.slice(1).some((f) => f.destroyed), false);
  const other = fake();
  listen('cave-cap-0', 'someone-else', asOut(other)).stop();
  assert.equal(other.destroyed, false, 'another member is not counted');
  assert.equal(listeners('cave-cap-0') + listeners('cave-cap-1') + listeners('cave-cap-2'), MAX_PER_MEMBER);
  for (const l of ls) l.stop();
});
