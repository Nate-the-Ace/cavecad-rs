import { CAVE_BLE_SERVICE, Command, LEG_BYTES, LegFilter, ackFor, decodeLeg, encodeLeg, hex } from '../src/lib/instrument/caveble.ts';
import { SimulatedInstrument, traverse } from '../src/lib/instrument/simulator.ts';
import type { LinkEvents, Reading } from '../src/lib/instrument/types.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const near = (a: number, b: number, e = 1e-4) => Math.abs(a - b) < e;

// A frame written out by hand from the reference client's layout (CaveBLE.kt): sequence byte 1, then
// four little-endian float32: azimuth 90.0 (00 00 b4 42), inclination -10.5 (00 00 28 c1), roll 0,
// distance 5.0 (00 00 a0 40). Not built with encodeLeg, so the decoder is checked against the spec.
const known = Uint8Array.from([0x01, 0x00, 0x00, 0xb4, 0x42, 0x00, 0x00, 0x28, 0xc1, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xa0, 0x40]);
const leg = decodeLeg(known);
ok(known.length === LEG_BYTES, 'a frame is 17 bytes');
ok(leg !== null && leg.seq === 1 && leg.azimuth === 90 && leg.inclination === -10.5 && leg.roll === 0 && leg.distance === 5, 'a hand-written frame decodes to its numbers');
ok(decodeLeg(known.subarray(0, 16)) === null && decodeLeg(new Uint8Array(18)) === null && decodeLeg(new Uint8Array(0)) === null, 'a frame of the wrong length is not a leg');
const badSeq = Uint8Array.from(known);
badSeq[0] = 2;
ok(decodeLeg(badSeq) === null, 'a sequence byte other than 0 or 1 is not a leg');
const nan = Uint8Array.from(known);
nan.set([0x00, 0x00, 0xc0, 0x7f], 13);
ok(decodeLeg(nan) === null, 'a NaN distance is not a leg');
ok(decodeLeg(new DataView(known.buffer.slice(0)))?.distance === 5, 'a DataView (what Web Bluetooth hands over) decodes too');

const round = decodeLeg(encodeLeg({ seq: 0, azimuth: 359.5, inclination: 12.25, roll: -3, distance: 17.125 }));
ok(round !== null && round.seq === 0 && round.azimuth === 359.5 && round.inclination === 12.25 && round.roll === -3 && round.distance === 17.125, 'encode then decode returns the same leg');

ok(ackFor(0) === 0x55 && ackFor(1) === 0x56 && Command.ACK === 0x55 && Command.TAKE_SHOT === 0x38 && Command.LASER_ON === 0x36, 'command bytes match the reference client');
ok(CAVE_BLE_SERVICE === '137c4435-8a64-4bcb-93f1-3792c6bdc965', 'the service uuid matches the reference client');
ok(hex(Uint8Array.from([0, 15, 255])) === '00 0f ff', 'frames are logged as hex');

const f = new LegFilter();
ok(f.isNew(known) && !f.isNew(Uint8Array.from(known)), 'the same frame twice is one shot');
const other = Uint8Array.from(known);
other[0] = 0;
ok(f.isNew(other) && f.isNew(known), 'a different frame is a new shot, and so is one after it');

// The simulator runs the real decode, acknowledge and repeat handling.
function collect(opts?: ConstructorParameters<typeof SimulatedInstrument>[1]) {
  const readings: Reading[] = [];
  const frames: string[] = [];
  const events: LinkEvents = { status() {}, frame: (h) => frames.push(h), reading: (r) => readings.push(r) };
  return { readings, frames, sim: new SimulatedInstrument(events, opts) };
}
const a = collect({ steps: [{ kind: 'leg', distance: 4.5, azimuth: 120, inclination: -3 }, { kind: 'back', distance: 4.5, azimuth: 300.5, inclination: 3 }] });
await a.sim.connect();
a.sim.takeShot();
a.sim.takeShot();
a.sim.takeShot(); // the script has run out: nothing
ok(a.readings.length === 2 && a.readings[0].distance === 4.5 && a.readings[0].azimuth === 120 && a.readings[1].azimuth === 300.5, 'two shots arrive as two readings, in order');
ok(a.readings[0].distance === 4.5 && a.readings[0].inclination === -3, 'exact values stay exact');
const f32 = collect({ steps: [{ kind: 'leg', distance: 8.84, azimuth: 333.6, inclination: 4.8 }] });
await f32.sim.connect();
f32.sim.takeShot();
ok(f32.readings[0].distance === 8.84 && f32.readings[0].azimuth === 333.6 && f32.readings[0].inclination === 4.8, 'the noise of a 32-bit float is rounded away: 8.84 is stored as 8.84');
ok(a.readings[0].source === 'sim' && a.readings[0].roll === 0, 'a reading says where it came from');
ok(a.sim.received.join() === '85,86', 'each leg is acknowledged with its own sequence bit (0x55 then 0x56)');

const b = collect({ repeatFrames: true, steps: traverse({ seed: 3, legs: 4 }) });
await b.sim.connect();
for (let i = 0; i < 4; i++) b.sim.takeShot();
ok(b.readings.length === 4, 'a frame sent twice (a lost ack) is one reading, not two');
ok(b.sim.received.length === 8, 'but every copy is acknowledged, or the instrument would never stop');

const c = collect({ steps: traverse({ seed: 3, legs: 2 }) });
ok(c.sim.upcoming !== null, 'the script is there before it is connected');
c.sim.takeShot();
ok(c.readings.length === 0, 'a disconnected instrument gives nothing');

// The traverse: same seed, same cave; backsights read the other way; splays follow their leg.
const t1 = JSON.stringify(traverse({ seed: 11, legs: 6, backsights: true, splays: 2 }));
ok(t1 === JSON.stringify(traverse({ seed: 11, legs: 6, backsights: true, splays: 2 })), 'a seed makes the same cave every time');
ok(t1 !== JSON.stringify(traverse({ seed: 12, legs: 6, backsights: true, splays: 2 })), 'another seed makes another cave');
const steps = traverse({ seed: 11, legs: 6, backsights: true, splays: 2 });
ok(steps.map((s) => s.kind).join(' ') === 'leg back splay splay '.repeat(6).trim(), 'order: foresight, backsight, splays');
const fs = steps[0];
const bs = steps[1];
ok(Math.abs(((((bs.azimuth - fs.azimuth - 180) % 360) + 540) % 360) - 180) < 1 && Math.abs(bs.inclination + fs.inclination) < 1, 'a backsight reads the opposite way within a degree');
ok(steps.every((s) => s.distance > 0 && s.inclination >= -90 && s.inclination <= 90 && s.azimuth >= 0 && s.azimuth < 360), 'every step is a believable reading');
ok(near(1, 1), 'sanity');

if (bad) process.exit(1);
console.log('caveble ok');
