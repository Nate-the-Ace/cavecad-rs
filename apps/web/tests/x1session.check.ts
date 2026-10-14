// A whole survey from a simulated X1, end to end with everything but the screen and the real engine:
// packets in, framing and acks, decoding, grouping into legs, the session's rules, the notebook writes.
import { LegAssembler, type LegEvent } from '../src/lib/instrument/legs.ts';
import { traverse } from '../src/lib/instrument/simulator.ts';
import { X1SimulatedInstrument, type FlagPolicy } from '../src/lib/instrument/x1sim.ts';
import type { LinkEvents, Reading } from '../src/lib/instrument/types.ts';
import { start, type SessionState } from '../src/lib/survey.ts';
import { run, type Entry, type Incoming } from '../src/lib/survey-run.ts';
import { fakeNotebook, type Row } from './fake-notebook.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const near = (a: number | null | undefined, b: number, e: number) => typeof a === 'number' && Math.abs(a - b) <= e;

interface Trip {
  rows: Row[];
  state: SessionState;
  log: Entry[];
  errors: string[];
  warnings: string[];
}

/** Fires every shot of the script through the simulator, the grouping and the session. */
async function survey(opts: { steps: ReturnType<typeof traverse>; policy: FlagPolicy; backsights: boolean; repeatFrames?: boolean; short?: boolean }): Promise<Trip> {
  const nb = fakeNotebook();
  const assembler = new LegAssembler();
  const trip: Trip = { rows: nb.rows, state: start('A1', new Set(), { mode: 'auto', backsights: opts.backsights }), log: [], errors: [], warnings: [] };
  const names = () => new Set(nb.rows.flatMap((r) => [r.from, r.to ?? '']));
  const handle = (incoming: Incoming) => {
    const out = run(nb.port, 't', trip.state, incoming, names, trip.log);
    if (!out.ok) {
      if (out.error) trip.errors.push(out.error);
      return;
    }
    trip.state = out.state;
    trip.warnings.push(...out.warnings);
    if ('entry' in out) trip.log.push({ ...out.entry, n: trip.log.length + 1 });
    else trip.log = trip.log.map((e) => (e.shot === out.updated.shot ? out.updated : e));
  };
  const events: LinkEvents = {
    status() {},
    frame() {},
    reading: (r: Reading) => assembler.push(r).forEach((e: LegEvent) => handle(e)),
  };
  const sim = new X1SimulatedInstrument(events, { steps: opts.steps, policy: opts.policy, repeatFrames: opts.repeatFrames, short: opts.short });
  await sim.connect();
  while (sim.upcoming) sim.takeShot();
  assembler.flush().forEach((e) => handle(e));
  return trip;
}

const withBacks = traverse({ seed: 5, legs: 6, backsights: true, splays: 2 });
for (const policy of ['all', 'third', 'none'] as FlagPolicy[]) {
  const t = await survey({ steps: withBacks, policy, backsights: true });
  const legs = t.rows.filter((r) => r.to !== null);
  const splays = t.rows.filter((r) => r.to === null);
  ok(t.errors.length === 0, `${policy}: nothing refused (${t.errors.join('; ')})`);
  ok(legs.map((r) => `${r.from}-${r.to}`).join(' ') === 'A1-A2 A2-A3 A3-A4 A4-A5 A5-A6 A6-A7', `${policy}: the legs run A1 to A7`);
  ok(splays.length === 12 && splays.every((r, i) => r.from === `A${Math.floor(i / 2) + 2}`), `${policy}: two splays at each new station`);
  const legSteps = withBacks.filter((s) => s.kind === 'leg');
  const backSteps = withBacks.filter((s) => s.kind === 'back');
  ok(legs.every((r, i) => near(r.cells.distance, legSteps[i].distance, 0.004) && near(r.cells.az_fs, legSteps[i].azimuth, 0.3) && near(r.cells.inc_fs, legSteps[i].inclination, 0.3)), `${policy}: each leg holds the surveyed foresight (the average of its three shots)`);
  ok(legs.every((r, i) => near(r.cells.az_bs, backSteps[i].azimuth, 0.3) && near(r.cells.inc_bs, backSteps[i].inclination, 0.3)), `${policy}: and its backsight, from the shots marked Backsight`);
  ok(legs.every((r) => r.cells.az_bs !== undefined) && splays.every((r) => r.cells.az_bs === undefined), `${policy}: only legs have backsights`);
  ok(t.state.from === 'A7' && t.state.awaiting === null, `${policy}: the surveyor ends standing at A7`);
  ok(t.log.length === 6 + 6 + 12, `${policy}: 24 things in the log`);
}

// A noisy link: every packet twice, and the 33-byte form. The notebook is the same.
{
  const a = await survey({ steps: withBacks, policy: 'third', backsights: true });
  const b = await survey({ steps: withBacks, policy: 'third', backsights: true, repeatFrames: true, short: true });
  ok(JSON.stringify(a.rows.map((r) => [r.from, r.to, r.cells])) === JSON.stringify(b.rows.map((r) => [r.from, r.to, r.cells])), 'repeated and short packets give the identical notebook');
}

// No backsights taken: legs, then splays at the new station; the next leg starts there.
{
  const steps = traverse({ seed: 8, legs: 4, backsights: false, splays: 2 });
  const t = await survey({ steps, policy: 'third', backsights: false });
  ok(t.rows.filter((r) => r.to !== null).map((r) => `${r.from}-${r.to}`).join(' ') === 'A1-A2 A2-A3 A3-A4 A4-A5', 'without backsights the legs run on');
  ok(t.rows.filter((r) => r.to === null).every((r, i) => r.from === `A${Math.floor(i / 2) + 2}`), 'and the splays are at the new stations');
  ok(t.rows.every((r) => r.cells.az_bs === undefined), 'no backsight cells filled');
}

// Backsights switched on but the surveyor skips them: the splay (or next leg) means they have moved on.
{
  const steps = traverse({ seed: 8, legs: 4, backsights: false, splays: 2 });
  const t = await survey({ steps, policy: 'third', backsights: true });
  ok(t.errors.length === 0, 'skipping backsights is not an error');
  ok(t.rows.filter((r) => r.to !== null).map((r) => `${r.from}-${r.to}`).join(' ') === 'A1-A2 A2-A3 A3-A4 A4-A5', 'the legs still run A1 to A5');
  ok(t.rows.filter((r) => r.to === null).every((r, i) => r.from === `A${Math.floor(i / 2) + 2}`), 'and the splays are at the stations they were taken at, not the one before');
}

// A backsight marked on the instrument with backsights switched off still goes to the leg before it.
{
  const t = await survey({ steps: withBacks, policy: 'third', backsights: false });
  const legs = t.rows.filter((r) => r.to !== null);
  ok(t.errors.length === 0 && legs.length === 6 && legs.every((r) => r.cells.az_bs !== undefined), 'a backsight marked on the X1 finds its leg without the Backsights switch');
  ok(t.rows.filter((r) => r.to === null).every((r, i) => r.from === `A${Math.floor(i / 2) + 2}`), 'and nothing is mislaid: the splays are still at the new stations');
}

// Refusals.
{
  const nb = fakeNotebook();
  const s0 = start('A1', new Set(), { mode: 'auto', backsights: false });
  const names = () => new Set<string>();
  const bs = run(nb.port, 't', s0, { kind: 'leg', group: 1, shots: 3, flagged: true, mark: 'backsight', spread: { distance: 0, azimuth: 0, inclination: 0 }, reading: { distance: 5, azimuth: 280, inclination: 3, roll: 0, source: 't', mark: 'backsight' } }, names, []);
  ok(!bs.ok && /needs a leg/.test(bs.ok ? '' : bs.error) && nb.rows.length === 0, 'a backsight with nothing before it is refused and writes nothing');
  const zero = run(nb.port, 't', s0, { kind: 'splay', reading: { distance: 0, azimuth: 280, inclination: 3, roll: 0, source: 't' } }, names, []);
  ok(!zero.ok && nb.rows.length === 0, 'a zero tape is refused');
  const single = run(nb.port, 't', { ...s0, mode: 'leg' }, { kind: 'single', reading: { distance: 4, azimuth: 10, inclination: 0, roll: 0, source: 't' } }, names, []);
  ok(single.ok && 'entry' in single && single.entry.label === 'A1 to A2' && nb.rows.length === 1, 'a single reading in manual mode is the next leg');
  const gone = run(nb.port, 't', s0, { kind: 'leg-update', group: 99, shots: 4, flagged: false, mark: 'none', spread: { distance: 0, azimuth: 0, inclination: 0 }, reading: { distance: 5, azimuth: 1, inclination: 1, roll: 0, source: 't' } }, names, []);
  ok(!gone.ok && gone.error === '', 'an update for a leg that was taken back is quietly dropped');
}

// An accuracy error and a loose leg are written, with a word about each.
{
  const nb = fakeNotebook();
  const s0 = start('A1', new Set(), { mode: 'auto', backsights: false });
  const out = run(nb.port, 't', s0, { kind: 'leg', group: 1, shots: 3, flagged: true, mark: 'none', spread: { distance: 0.05, azimuth: 1.9, inclination: 0.2 }, reading: { distance: 5, azimuth: 100, inclination: 3, roll: 0, source: 't', error: 'accuracy error: absG' } }, () => new Set(), []);
  ok(out.ok && out.warnings.some((w) => /accuracy error/.test(w)) && out.warnings.some((w) => /differ by/.test(w)) && nb.rows.length === 1, 'an accuracy error and a loose leg are both written, and both said');
}

if (bad) process.exit(1);
console.log('x1session ok');
