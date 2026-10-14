// Writes what the web app's own instrument code (the TypeScript the Rust crate `cave-instrument` replaces)
// gives for a spread of wire frames, link sequences, leg groupings and simulated surveys, for the Rust copy to
// match exactly:
//   node --experimental-strip-types tools/golden/gen_instrument_golden.mjs <out.json>
// Run it against the commit that still has apps/web/src/lib/instrument/*.ts (survey-sketch before the port).
import { writeFileSync } from 'node:fs';
import { encodeLeg, decodeLeg } from '../../apps/web/src/lib/instrument/caveble.ts';
import { encodePacket, decodePacket } from '../../apps/web/src/lib/instrument/cavway.ts';
import { encodeDistoXPacket, decodeDistoXPacket } from '../../apps/web/src/lib/instrument/sap5.ts';
import { encodeShot, decodePrimary, decodeMeta, decodeErrors, describeBricErrors } from '../../apps/web/src/lib/instrument/bric.ts';
import { BricLink, LegLink, Sap5Link, X1Link } from '../../apps/web/src/lib/instrument/link.ts';
import { LegAssembler, average, isClose } from '../../apps/web/src/lib/instrument/legs.ts';
import { rng, traverse } from '../../apps/web/src/lib/instrument/simulator.ts';
import { DEVICES, SimulatedInstrument, shotsFor } from '../../apps/web/src/lib/instrument/devsim.ts';
import { identify } from '../../apps/web/src/lib/instrument/identify.ts';

const hex = (b) => Array.from(b, (x) => x.toString(16).padStart(2, '0')).join(' ');
const fromHex = (h) => Uint8Array.from(h.split(' ').filter(Boolean).map((x) => parseInt(x, 16)));
const out = {};

// ---- CaveBLE ----
{
  const legs = [
    { seq: 0, azimuth: 123.456, inclination: -12.5, roll: 3.2, distance: 8.84 },
    { seq: 1, azimuth: 0, inclination: 90, roll: 0, distance: 0.05 },
    { seq: 0, azimuth: 359.999, inclination: -89.5, roll: -170, distance: 123.456 },
  ];
  const frames = legs.map((l) => ({ leg: l, hex: hex(encodeLeg(l)), decoded: decodeLeg(encodeLeg(l)) }));
  const bad = [fromHex('00 01'), (() => { const b = encodeLeg(legs[0]); b[0] = 2; return b; })(), (() => { const b = encodeLeg(legs[0]); b.set([0, 0, 0xc0, 0x7f], 1); return b; })()];
  out.caveble = { frames, bad: bad.map((b) => ({ hex: hex(b), decoded: decodeLeg(b) })) };
}

// ---- Cavway X1 ----
{
  const specs = [
    { distance: 8.84, azimuth: 123.4, inclination: -12.5 },
    { distance: 0.5, azimuth: 0, inclination: 90, leg: true },
    { distance: 0.5, azimuth: 359.99, inclination: -90, leg: true, mark: 'backsight' },
    { distance: 12.345, azimuth: 270, inclination: 45, mark: 'feature', roll: 33, time: 1700000123 },
    { distance: 100.001, azimuth: 45.5, inclination: 0, mark: 'ridge', short: true },
    { distance: 3, azimuth: 10, inclination: 3, mark: 'generic' },
    { distance: 0, azimuth: 295, inclination: 8, type: 2, time: 99 },
    { distance: 6.5, azimuth: 77, inclination: 5, errors: [0x6f, 0x04, 0x29, 0x1c, 0x25, 0x1c, 0x07, 0x00, 0x00] },
    { distance: 6.5, azimuth: 77, inclination: 5, errors: [0x3f, 0x10, 0x20, 0x30, 0x40, 0x50, 0x60, 0x70, 0x80] },
    { distance: 6.5, azimuth: 77, inclination: 5, errors: [0xe7, 0xff, 0x7f, 0x01, 0x80, 0, 0, 0, 0] },
  ];
  const next = rng(99);
  const random = [];
  for (let i = 0; i < 40; i++) {
    const n = i % 5 === 0 ? 33 : 64;
    const b = Uint8Array.from({ length: n }, () => Math.floor(next() * 256));
    b[0] = 1 + (i % 2);
    if (i % 3 === 0) b[45] = 0xff;
    random.push(b);
  }
  const cases = [
    ...specs.map((s) => ({ spec: s, hex: hex(encodePacket(s)) })),
    ...random.map((b) => ({ hex: hex(b) })),
    { hex: hex(fromHex('01 ff')) },
    { hex: hex(Uint8Array.from({ length: 64 }, () => 0xff)) },
    { hex: hex(Uint8Array.from({ length: 64 }, (_, i) => (i === 0 ? 3 : 0))) },
  ];
  for (const c of cases) c.decoded = decodePacket(fromHex(c.hex));
  out.cavway = cases;
}

// ---- SAP5 ----
{
  const specs = [
    { distance: 8.84, azimuth: 123.4, inclination: -12.5, roll: 3, seq: 0 },
    { distance: 40, azimuth: 300, inclination: 80, roll: 0, seq: 1 },
    { distance: 70, azimuth: 10, inclination: -80, roll: 100, seq: 1 },
    { distance: 0.1, azimuth: 0, inclination: 0, seq: 0 },
  ];
  const next = rng(5);
  const cases = specs.map((s) => ({ spec: s, hex: hex(encodeDistoXPacket(s)) }));
  for (let i = 0; i < 40; i++) cases.push({ hex: hex(Uint8Array.from({ length: 8 }, (_, k) => (k === 0 ? 0x01 | (Math.floor(next() * 4) << 6) : Math.floor(next() * 256)))) });
  cases.push({ hex: '02 00 00 00 00 00 00 00' }, { hex: '01 00 00' });
  for (const c of cases) c.decoded = decodeDistoXPacket(fromHex(c.hex));
  out.sap5 = cases;
}

// ---- BRIC ----
{
  const specs = [
    { distance: 8.84, azimuth: 123.4, inclination: -12.5, index: 1 },
    { distance: 0.5, azimuth: 0, inclination: 90, index: 2, roll: 10, dip: 55, type: 1, time: { year: 2026, month: 3, day: 4, hour: 5, minute: 6, second: 7, centisecond: 8 } },
    { distance: 3, azimuth: 200, inclination: 4, index: 9, errors: [[7, 1.5, 2.5], [8, 0, 0]] },
    { distance: 3, azimuth: 200, inclination: 4, index: 10, errors: [[99, 0, 0]] },
  ];
  const cases = specs.map((s) => {
    const e = encodeShot(s);
    const eb = decodeErrors(e.errors);
    return { spec: s, primary: hex(e.primary), meta: hex(e.meta), errors: hex(e.errors), decoded: { primary: decodePrimary(e.primary), meta: decodeMeta(e.meta), errors: eb, said: describeBricErrors(eb) } };
  });
  const short = [fromHex('01 02 03'), new Uint8Array(19), new Uint8Array(9)];
  out.bric = { cases, short: short.map((b) => ({ hex: hex(b), primary: decodePrimary(b), meta: decodeMeta(b), errors: decodeErrors(b) })) };
}

// ---- links ----
function collect() {
  const log = [];
  return {
    log,
    events: { status() {}, frame: (h, f) => log.push({ k: 'frame', hex: h, from: f }), reading: (r) => log.push({ k: 'reading', reading: r }) },
    sendByte: (b) => log.push({ k: 'send', hex: hex([b]) }),
    sendBytes: (b) => log.push({ k: 'send', hex: hex(b) }),
  };
}
{
  // SAP6
  const c = collect();
  const link = new LegLink(c.events, c.sendByte, 'sim');
  const f0 = encodeLeg({ seq: 0, azimuth: 100, inclination: 3, roll: 0, distance: 5.5 });
  const f1 = encodeLeg({ seq: 1, azimuth: 101, inclination: 4, roll: 0, distance: 5.6 });
  for (const f of [f0, f0, f1, fromHex('01 02'), f1, f0]) link.onFrame(f, 'leg');
  const sap6 = { frames: [f0, f0, f1, fromHex('01 02'), f1, f0].map(hex), log: c.log };

  // X1
  const x = collect();
  const x1 = new X1Link(x.events, x.sendBytes, 'sim');
  const p = (s) => encodePacket(s);
  const seq = [
    p({ distance: 5, azimuth: 100, inclination: 1, time: 10 }),
    p({ distance: 5, azimuth: 100, inclination: 1, time: 10 }),
    p({ distance: 5, azimuth: 100, inclination: 1, time: 11, leg: true, mark: 'backsight' }),
    p({ distance: 0, azimuth: 295, inclination: 8, type: 2, time: 12 }),
    p({ distance: 6, azimuth: 10, inclination: -1, time: 13, short: true }),
    p({ distance: 6, azimuth: 10, inclination: -1, time: 14, errors: [0x6f, 0x04, 0x29, 0x1c, 0x25, 0x1c, 0x07, 0x00, 0x00] }),
    fromHex('01 02 03'),
    Uint8Array.from({ length: 40 }, (_, i) => (i === 0 ? 9 : 0)),
  ];
  for (const f of seq) x1.onFrame(f, 'x1');
  for (let i = 0; i < 10; i++) x1.onFrame(p({ distance: 7 + i, azimuth: 1, inclination: 1, time: 100 + i }), 'x1');
  x1.onFrame(seq[0], 'x1'); // long forgotten by now: a new shot again
  out.x1link = { frames: [...seq.map(hex)], log: x.log };
  out.sap6link = sap6;

  // SAP5
  const s5 = collect();
  const l5 = new Sap5Link(s5.events, 'sim');
  const q = [
    encodeDistoXPacket({ distance: 5, azimuth: 100, inclination: 2, seq: 0 }),
    encodeDistoXPacket({ distance: 5, azimuth: 100, inclination: 2, seq: 0 }),
    encodeDistoXPacket({ distance: 6, azimuth: 110, inclination: 2, seq: 1 }),
    fromHex('02 00 00 00 00 00 00 00'),
    encodeDistoXPacket({ distance: 40, azimuth: 110, inclination: 2, seq: 0 }),
  ];
  for (const f of q) l5.onFrame(f, 'sap5');
  out.sap5link = { frames: q.map(hex), log: s5.log };

  // BRIC (flush by hand: no clock)
  const timers = { schedule: () => 0, cancel: () => {} };
  const b = collect();
  const bl = new BricLink(b.events, 'sim', timers);
  const steps = [];
  const shot = (s) => encodeShot(s);
  const play = (kind, bytes) => { steps.push({ op: 'frame', kind, hex: hex(bytes) }); bl.onFrame(kind, bytes); };
  const flush = () => { steps.push({ op: 'flush' }); bl.flush(); };
  const a = shot({ distance: 5, azimuth: 100, inclination: 2, index: 1 });
  play('primary', a.primary); play('meta', a.meta); play('errors', a.errors);
  play('primary', a.primary); // repeat
  const c2 = shot({ distance: 6, azimuth: 101, inclination: 3, index: 2 });
  play('primary', c2.primary); play('meta', c2.meta); // errors never come
  flush();
  const d = shot({ distance: 7, azimuth: 102, inclination: 4, index: 5, errors: [[7, 0, 0]] }); // missed 2
  play('primary', d.primary); play('meta', d.meta); play('errors', d.errors);
  const e = shot({ distance: 0, azimuth: 102, inclination: 4, index: 6 }); // no length
  play('primary', e.primary); play('meta', e.meta); play('errors', e.errors);
  const f = shot({ distance: 8, azimuth: 103, inclination: 5, index: 7 });
  play('primary', f.primary); // left behind by the next primary
  const g = shot({ distance: 9, azimuth: 104, inclination: 6, index: 8 });
  play('primary', g.primary); play('meta', g.meta); play('errors', g.errors);
  play('primary', new Uint8Array(5)); play('meta', new Uint8Array(5)); play('errors', new Uint8Array(5));
  flush();
  out.briclink = { steps, log: b.log };
}

// ---- legs ----
{
  const R = (distance, azimuth, inclination, extra = {}) => ({ distance, azimuth, inclination, roll: 0, source: 't', ...extra });
  const cases = {};
  const run = (name, readings, flushAt = []) => {
    const a = new LegAssembler();
    const events = [];
    readings.forEach((r, i) => {
      events.push(...a.push(r));
      if (flushAt.includes(i)) events.push({ flush: a.flush() });
    });
    events.push({ flush: a.flush() });
    cases[name] = { readings, flushAt, events, waiting: a.waiting };
  };
  for (const policy of ['all', 'third', 'none']) {
    const steps = traverse({ seed: 31, legs: 4, backsights: true, splays: 4, startSplays: true, lrud: false });
    const shots = shotsFor(steps, policy).map((s) => R(s.distance, s.azimuth, s.inclination, { leg: s.leg, mark: s.mark }));
    run(`policy-${policy}`, shots);
  }
  const steps = traverse({ seed: 3, legs: 3, backsights: false, splays: 4, lrud: true });
  run('lrud', shotsFor(steps, 'third').map((s) => R(s.distance, s.azimuth, s.inclination, { leg: s.leg, mark: s.mark, ...(s.lrud ? { lrud: s.lrud } : {}) })));
  run('steep', [R(8, 10, 88), R(8, 200, 88.5), R(8.01, 100, 89), R(8, 10, 80)]);
  run('wrap', [R(5, 359.5, 0), R(5, 0.8, 0), R(5, 0.1, 0), R(5, 0.2, 0.1)]);
  run('flush-mid', [R(5, 100, 0), R(5, 100.1, 0), R(9, 200, 30), R(9, 200, 30.1)], [1]);
  run('errors-marks', [R(3, 10, 0, { error: '', mark: 'none' }), R(3, 10, 0, { error: 'accuracy error: absG', mark: 'backsight' }), R(3, 10, 0, { error: 'x', mark: 'ridge' })]);
  // The edges of "close": each pair sits a hair either side of a limit.
  run('edge-azimuth', [R(5, 100, 0), R(5, 101.9, 0), R(5, 100, 0), R(5, 102.1, 0), R(5, 100, 0), R(5, 102.6, 0)]);
  run('edge-clino', [R(5, 100, 0), R(5, 100, 1.9), R(5, 100, 0), R(5, 100, 2.1), R(5, 100, 0), R(5, 100, 2.6)]);
  run('edge-tape', [R(5, 100, 0), R(5.049, 100, 0), R(5, 100, 0), R(5.051, 100, 0), R(100, 100, 0), R(100.49, 100, 0), R(100, 100, 0), R(100.6, 100, 0)]);
  out.legs = cases;
  const pairs = [[R(5, 100, 2), R(5.04, 101.5, 3.5)], [R(5, 100, 2), R(5.2, 100, 2)], [R(5, 100, 2), R(5, 104, 2)], [R(100, 100, 2), R(100.4, 100, 2)], [R(8, 10, 88), R(8, 200, 88.5)], [R(8, 10, 88), R(8, 200, 80)]];
  out.close = pairs.map(([a, b]) => ({ a, b, close: isClose(a, b) }));
  out.average = [[R(5, 359.9, 0), R(5, 0.1, 0), R(5, 0, 0)], [R(5, 100, 10), R(5.02, 101, 11), R(5.01, 99, 12)], [R(3, 10, 89), R(3.01, 200, 89.5)]].map((s) => ({ shots: s, ...average(s) }));
}

// ---- identify ----
{
  const cases = [['CavwayX1-1234', []], ['SAP6_ab', []], ['SAP_x', []], ['SAP5_a', []], ['Shetland_q', []], ['BRIC4_x', []], ['BRIC5-9', []], ['BRIC-', []], ['DistoXBLE-1', []], ['Whatever', ['137C4435-8A64-4BCB-93F1-3792C6BDC965']], ['', ['00000058d0-0000-1000-8000-00805f9b34fb'.replace('00000058d0', '000058d0')]], ['', ['6e400001-b5a3-f393-e0a9-e50e24dcca9e']], ['  CavwayX1-7', []]];
  out.identify = cases.map(([name, services]) => ({ name, services, result: identify(name, services) }));
}

// ---- simulator ----
{
  const opts = [
    { seed: 7, legs: 20, backsights: true, splays: 2 },
    { seed: 31, legs: 3, backsights: true, splays: 4, startSplays: true, lrud: true },
    { seed: 1, legs: 5, backsights: false, splays: 3, startSplays: true },
    { seed: 99, legs: 4, backsights: true, splays: 0 },
    { seed: 4000000000, legs: 2, backsights: true, splays: 4, lrud: true },
  ];
  out.traverse = opts.map((o) => ({ opts: o, steps: traverse(o) }));
  const steps = traverse(opts[1]);
  out.shotsFor = ['all', 'third', 'none'].flatMap((policy) => [1, 5].map((seed) => ({ policy, seed, steps, shots: shotsFor(steps, policy, seed) })));
  const runs = [];
  for (const d of DEVICES) {
    for (const quirk of d.quirks.map((q) => q.id)) {
      for (const wantBack of [true, false]) {
        const log = [];
        const events = { status() {}, frame: (h, f) => log.push({ k: 'frame', hex: h, from: f }), reading: (r) => log.push({ k: 'reading', reading: r }) };
        const sim = new SimulatedInstrument(events, { kind: d.kind, steps, quirks: quirk === 'none' ? [] : [quirk] });
        sim.wantBacksights = () => wantBack;
        sim.wantLrud = () => true;
        await sim.connect();
        const fired = [];
        for (let i = 0; i < 6; i++) fired.push(sim.takeStation());
        sim.disconnect();
        runs.push({ kind: d.kind, quirk, wantBack, fired, log, received: sim.received.map((b) => hex(typeof b === 'number' ? [b] : b)) });
      }
    }
  }
  // Single shots too (no station grouping), including a BRIC left waiting
  for (const d of DEVICES) {
    const log = [];
    const events = { status() {}, frame: (h, f) => log.push({ k: 'frame', hex: h, from: f }), reading: (r) => log.push({ k: 'reading', reading: r }) };
    const sim = new SimulatedInstrument(events, { kind: d.kind, steps, quirks: [] });
    await sim.connect();
    for (let i = 0; i < 12; i++) sim.takeShot();
    sim.disconnect();
    runs.push({ kind: d.kind, quirk: 'single', wantBack: true, fired: [12], log, received: sim.received.map((b) => hex(typeof b === 'number' ? [b] : b)) });
  }
  out.sim = runs;
  out.devices = DEVICES;
}

writeFileSync(process.argv[2], JSON.stringify(out));
console.log('instrument golden:', Object.keys(out).join(' '));
