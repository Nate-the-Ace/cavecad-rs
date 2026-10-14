// Every simulated device, with each of its quirks, run through its real wire format and real decoder into a
// stand-in notebook. A quirk should either change nothing about the survey (the logic copes) or change exactly what
// it is meant to (a warning, a log line, a corrected distance). All four devices must also agree on the same survey.
import { LegAssembler, type LegEvent } from '../src/lib/instrument/legs.ts';
import { traverse } from '../src/lib/instrument/simulator.ts';
import { DEVICES, SimulatedInstrument, deviceOf, type DeviceKind } from '../src/lib/instrument/devsim.ts';
import type { LinkEvents, Reading } from '../src/lib/instrument/types.ts';
import { start, type SessionState } from '../src/lib/survey.ts';
import { run, type Entry, type Incoming } from '../src/lib/survey-run.ts';
import { fakeNotebook } from './fake-notebook.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const near = (a: number | null | undefined, b: number, e: number) => typeof a === 'number' && Math.abs(a - b) <= e;

const STEPS = traverse({ seed: 31, legs: 3, backsights: true, splays: 4, startSplays: true, lrud: true });

async function survey(kind: DeviceKind, quirks: string[] = [], backsights = true) {
  const nb = fakeNotebook();
  const assembler = new LegAssembler();
  let state: SessionState = start('A1', new Set(), { mode: 'auto', backsights });
  let log: Entry[] = [];
  const errors: string[] = [];
  const warnings: string[] = [];
  const frames: string[] = [];
  const names = () => new Set(nb.rows.flatMap((r) => [r.from, r.to ?? '']));
  const handle = (incoming: Incoming) => {
    const out = run(nb.port, 't', state, incoming, names, log);
    if (!out.ok) {
      if (out.error) errors.push(out.error);
      return;
    }
    state = out.state;
    warnings.push(...out.warnings);
    if ('entry' in out) log.push({ ...out.entry, n: log.length + 1 });
    else log = log.map((e) => (e.shot === out.updated.shot ? out.updated : e));
  };
  const events: LinkEvents = { status() {}, frame: (h) => frames.push(h), reading: (r: Reading) => assembler.push(r).forEach((e: LegEvent) => handle(e)) };
  const sim = new SimulatedInstrument(events, { kind, steps: STEPS, quirks });
  sim.wantBacksights = () => backsights;
  sim.onBurst = () => assembler.flush().forEach((e) => handle(e));
  await sim.connect();
  const fired = [sim.takeStation(), sim.takeStation(), sim.takeStation(), sim.takeStation()];
  sim.disconnect();
  return { rows: nb.rows, walls: nb.walls, errors, warnings, frames, sim, fired, state };
}

/** The notebook as data, to compare two surveys. */
const book = (rows: { from: string; to: string | null; cells: object }[]) => JSON.stringify(rows.map((r) => [r.from, r.to, r.cells]));
const legsOf = (rows: { from: string; to: string | null }[]) => rows.filter((r) => r.to !== null).map((r) => `${r.from}-${r.to}`).join(' ');

const base: Record<string, Awaited<ReturnType<typeof survey>>> = {};
for (const d of DEVICES) {
  const b = await survey(d.kind);
  base[d.kind] = b;
  ok(b.errors.length === 0, `${d.label}: a clean survey is refused nothing (${b.errors.join('; ')})`);
  ok(legsOf(b.rows) === 'A1-A2 A2-A3 A3-A4', `${d.label}: three legs, A1 to A4`);
  ok(b.rows.filter((r) => r.to === null).length === 0, `${d.label}: the wall shots are not splays: no splay rows at all`);
  ok(Object.keys(b.walls).length === 16 && Object.values(b.walls).every((v) => typeof v === 'number' && v > 0), `${d.label}: sixteen wall dimensions: left, right, up and down at each of the four stations`);
  ok(b.rows.filter((r) => r.to !== null).every((r) => typeof (r.cells as Record<string, number>).az_bs === 'number'), `${d.label}: every leg has its backsight`);
  ok(b.fired.join() === '14,10,10,0', `${d.label}: the first click is four splays, a leg, its backsight and four splays (14); then ten; then nothing`);
  ok(b.state.from === 'A4' && b.state.awaiting === null, `${d.label}: the surveyor ends at A4`);
}

// All four devices tell the same story, to within what each one can say.
const x1 = base.x1.rows;
for (const d of DEVICES.filter((x) => x.kind !== 'x1')) {
  const rows = base[d.kind].rows;
  let agree = rows.length === x1.length;
  rows.forEach((r, i) => {
    const a = r.cells as Record<string, number | null | undefined>;
    const b = x1[i]?.cells as Record<string, number | null | undefined>;
    if (!b || r.from !== x1[i].from || r.to !== x1[i].to) { agree = false; return; }
    for (const k of ['distance', 'az_fs', 'inc_fs', 'az_bs', 'inc_bs']) if (typeof a[k] === 'number' || typeof b[k] === 'number') if (!near(a[k] as number, b[k] as number, k === 'distance' ? 0.01 : 0.05)) agree = false;
  });
  ok(agree, `${d.label} and the Cavway X1 write the same survey (distances to 1 cm, angles to 0.05 degrees)`);
  ok(Object.keys(base.x1.walls).every((k) => near(base[d.kind].walls[k], base.x1.walls[k] as number, 0.01)), `${d.label} and the Cavway X1 write the same wall dimensions`);
}

// Quirks that must change nothing about the survey.
const same: [DeviceKind, string][] = [
  ['x1', 'flag-all'], ['x1', 'flag-none'], ['x1', 'short'], ['x1', 'lost-acks'], ['x1', 'calibration'], ['x1', 'accuracy'],
  ['sap6', 'lost-acks'], ['sap5', 'repeats'], ['bric', 'no-errors'], ['bric', 'missed'], ['bric', 'scan'], ['bric', 'zero'], ['bric', 'errors'], ['bric', 'repeats'],
];
for (const [kind, q] of same) {
  const r = await survey(kind, [q]);
  const label = `${deviceOf(kind).label} / ${q}`;
  ok(r.errors.length === 0, `${label}: nothing is refused (${r.errors.join('; ')})`);
  ok(book(r.rows) === book(base[kind].rows), `${label}: the survey is the same as without the quirk`);
  ok(JSON.stringify(r.walls) === JSON.stringify(base[kind].walls), `${label}: and so are its wall dimensions`);
}

// And the ones that must show something.
{
  const r = await survey('x1', ['lost-acks']);
  ok(r.sim.received.length === 2 * (r.fired[0] + r.fired[1] + r.fired[2]), 'X1 lost acks: every repeat of every packet is acknowledged (twice as many acks as shots)');
  const c = await survey('x1', ['calibration']);
  ok(c.frames.some((f) => /calibration shot: acknowledged/.test(f)), 'X1 calibration: the log says a calibration shot was acknowledged and not surveyed');
  ok(c.sim.received.length > c.fired[0] + c.fired[1] + c.fired[2], 'X1 calibration: and the calibration packets were acknowledged too');
  const a = await survey('x1', ['accuracy']);
  ok(a.warnings.some((w) => /accuracy error/.test(w)), 'X1 accuracy: the warning reaches the surveyor');
  const six = await survey('sap6', ['lost-acks']);
  ok(six.sim.received.length === 2 * (six.fired[0] + six.fired[1] + six.fired[2]) && six.sim.received.every((b) => b[0] === 0x55 || b[0] === 0x56), 'SAP6 lost acks: each repeat is acknowledged again with 0x55 or 0x56');
  const none = base.sap6.sim.received;
  ok(none.length === base.sap6.fired[0] + base.sap6.fired[1] + base.sap6.fired[2] && none.map((b) => b[0]).join() === none.map((_, i) => 0x55 + (i % 2)).join(), 'SAP6: acknowledgements alternate 0x55, 0x56 with the sequence bit');
  const missed = await survey('bric', ['missed']);
  ok(missed.frames.some((f) => /missed 1 shot/.test(f)), 'BRIC missed shots: the log says so');
  const zero = await survey('bric', ['zero']);
  ok(zero.frames.some((f) => /no length/.test(f)), 'BRIC zero-length: the log says one was ignored');
  const err = await survey('bric', ['errors']);
  ok(err.warnings.some((w) => /the target moved/.test(w)), 'BRIC errors: "the target moved" reaches the surveyor');
  const quiet = await survey('bric', ['no-errors']);
  ok(!quiet.frames.some((f) => f === 'errors'), 'BRIC without errors: no error indication was ever sent');
}

// The SAP5 firmware fault: long shots, bit 16 set past 32.768 m, must still read their true length.
{
  const long = await survey('sap5', ['fault32']);
  const plain = base.sap5.rows.filter((r) => r.to !== null).map((r) => (r.cells as Record<string, number>).distance);
  const got = long.rows.filter((r) => r.to !== null).map((r) => (r.cells as Record<string, number>).distance);
  ok(got.length === 3 && got.every((d, i) => near(d, plain[i] * 8, 0.02)), 'SAP5 fault: every long leg reads eight times the short survey, to within 2 cm (the fault corrected)');
  ok(got.some((d) => d >= 32.768 && d < 65.536), 'SAP5 fault: and at least one leg is in the range where the firmware fault applies');
  const faulty = long.frames.some((f) => {
    const b = f.split(' ').map((h) => parseInt(h, 16));
    return b.length === 8 && (b[0] & 0x40) !== 0 && b[2] >= 0x80;
  });
  ok(faulty, 'SAP5 fault: the packets really did carry the fault (bit 16 set with the next byte\'s top bit set)');
  ok(long.errors.length === 0, 'SAP5 fault: nothing refused');
}

// With backsights off, no device writes any.
for (const d of DEVICES) {
  const r = await survey(d.kind, [], false);
  ok(r.errors.length === 0 && r.rows.filter((x) => x.to !== null).every((x) => (x.cells as Record<string, number | undefined>).az_bs === undefined), `${d.label}: with backsights off, no leg has one`);
  ok(r.fired.join() === '11,7,7,0', `${d.label}: and each station is three shots (the backsight) fewer`);
}

if (bad) process.exit(1);
console.log('devsim ok');
