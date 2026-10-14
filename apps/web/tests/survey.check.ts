import { advance, applied, standing, backsightWarnings, checkReading, decide, nextStep, skipBack, start, withMode, type Plan, type SessionState } from '../src/lib/survey.ts';
import { traverse } from '../src/lib/instrument/simulator.ts';
import type { Reading } from '../src/lib/instrument/types.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const r = (distance: number, azimuth: number, inclination: number): Reading => ({ distance, azimuth, inclination, roll: 0, source: 'test' });
const none = new Set<string>();

// Readings that cannot be written.
ok(!checkReading(r(0, 10, 0)).ok && !checkReading(r(-1, 10, 0)).ok, 'a zero or negative tape is refused');
ok(!checkReading(r(601, 10, 0)).ok && checkReading(r(599, 10, 0)).ok, 'a tape past 600 m is a glitch');
ok(!checkReading(r(5, 10, 91)).ok && !checkReading(r(5, 10, -90.5)).ok && checkReading(r(5, 10, 90)).ok, 'a clino past vertical is refused, vertical itself is fine');
ok(!checkReading(r(NaN, 10, 0)).ok && !checkReading(r(5, Infinity, 0)).ok, 'a reading that is not a number is refused');
const steep = checkReading(r(5, 10, 88));
ok(steep.ok && steep.warnings.length === 1 && /pitch/.test(steep.warnings[0]), 'a steep shot is kept and flagged as a pitch (the engine ignores its compass)');
const tiny = checkReading(r(0.02, 10, 0));
ok(tiny.ok && tiny.warnings.length === 1, 'a very short shot is kept and flagged');

// Backsights are compared the way the notebook flags a page.
ok(backsightWarnings(r(5, 90, 10), r(5, 270.5, -10.2)).length === 0, 'a good backsight says nothing');
ok(backsightWarnings(r(5, 350, 0), r(5, 170.5, 0)).length === 0, 'the compass comparison wraps round north');
ok(backsightWarnings(r(5, 90, 0), r(5, 280, 0)).some((w) => /Compass/.test(w)), 'ten degrees out on the compass is said');
ok(backsightWarnings(r(5, 90, 10), r(5, 270, 5)).some((w) => /Clino/.test(w)), 'a clino that does not mirror is said');
ok(backsightWarnings(r(5, 90, 0), r(5.5, 270, 0)).some((w) => /Tape/.test(w)), 'half a metre of tape difference is said');
ok(backsightWarnings(r(50, 90, 0), r(50.8, 270, 0)).length === 0, 'a tape difference within 2 % of a long shot is not');
ok(backsightWarnings(r(5, 10, 88), r(5, 200, -88)).length === 0, 'on a pitch the compass is not compared');

// Deciding.
const s0 = start('A1', none);
ok(s0.from === 'A1' && s0.to === 'A2' && s0.mode === 'leg' && !s0.backsights && s0.awaiting === null, 'a session starts at a station and names the next');
const d1 = decide(s0, r(4.2, 100, -5));
ok(d1.plan?.kind === 'leg' && d1.plan.from === 'A1' && d1.plan.to === 'A2' && d1.reject === null, 'a reading is the next leg');
ok(decide({ ...s0, from: '' }, r(4, 0, 0)).reject !== null && decide({ ...s0, to: ' ' }, r(4, 0, 0)).reject !== null, 'a leg needs both stations named');
ok(decide({ ...s0, to: 'A1' }, r(4, 0, 0)).reject !== null, 'a leg cannot start and end at one station');
ok(decide(s0, r(0, 0, 0)).plan === null, 'a refused reading writes nothing');
const sp = decide(withMode(s0, 'splay', none), r(1.1, 40, 0));
ok(sp.plan?.kind === 'splay' && sp.plan.from === 'A1', 'in splay mode a reading is a splay from where the surveyor stands');
ok(decide(withMode({ ...s0, to: '' }, 'splay', none), r(1, 0, 0)).plan?.kind === 'splay', 'a splay needs no destination');

// Moving on.
const asLeg = (d: ReturnType<typeof decide>): Plan => d.plan as Plan;
const s1 = applied(s0, asLeg(d1), 'shot1', none);
ok(s1.from === 'A2' && s1.to === 'A3' && s1.awaiting === null, 'after a leg (no backsights) they stand at the new station and the next name follows');
ok(applied(withMode(s0, 'splay', none), asLeg(sp), 'splay1', none).from === 'A1', 'a splay does not move them');
ok(advance({ ...s0, to: 'A2' }, new Set(['A3', 'A4'])).to === 'A5', 'the next name skips taken stations, so a loop is never closed by accident');

// Backsights.
const sb = { ...s0, backsights: true };
const fore = decide(sb, r(5, 90, 10));
const w1 = applied(sb, asLeg(fore), 'shotA', none);
ok(w1.awaiting?.shot === 'shotA' && w1.from === 'A1' && w1.to === 'A2', 'with backsights, they wait at the new station for the backsight');
ok(/backsight/i.test(nextStep(w1)) && /A2/.test(nextStep(w1)), 'and are told so');
const back = decide(w1, r(5, 270.2, -10.1));
ok(back.plan?.kind === 'back' && back.plan.shot === 'shotA' && back.warnings.length === 0, 'the next reading is that shot\'s backsight, whatever the mode');
ok(decide({ ...w1, mode: 'splay' }, r(5, 270, -10)).plan?.kind === 'back', 'a state still awaiting its backsight takes it, whatever the mode field says');
ok(decide(w1, r(5, 200, -10)).warnings.some((w) => /Compass/.test(w)), 'a bad backsight is written anyway, with a warning');
const w2 = applied(w1, asLeg(back), 'shotA', none);
ok(w2.from === 'A2' && w2.to === 'A3' && w2.awaiting === null, 'the backsight in, they move on');
const skipped = skipBack(w1, none);
ok(skipped.from === 'A2' && skipped.to === 'A3' && skipped.awaiting === null, 'skipping the backsight moves on without it');
ok(skipBack(s0, none) === s0, 'skipping with nothing awaited changes nothing');
const sw = withMode(w1, 'splay', none);
ok(sw.awaiting === null && sw.from === 'A2' && sw.to === 'A3' && sw.mode === 'splay', 'changing mode while a backsight is awaited gives it up and moves on: the splay is at the new station');
ok(withMode(s0, 'leg', none) === s0, 'choosing the mode already chosen changes nothing');
ok(withMode(s0, 'splay', none).from === 'A1', 'changing mode with nothing awaited does not move them');
ok(standing(s0) === 'A1' && standing(w1) === 'A2', 'they stand at the new station while its backsight is awaited');
ok(start('A5', none, { mode: 'splay', backsights: true }).backsights && start('A5', none, { mode: 'splay', backsights: true }).mode === 'splay', 'a restart can keep the surveyor\'s choices');
ok(start('ENT', none).to === '', 'a name with no number has no suggestion: the surveyor types it');
ok(/Name/.test(nextStep(start('ENT', none))), 'and is told to');

// A whole trip through a stand-in notebook, driven by the simulator's traverse: legs with backsights and
// splays. What the notebook ends up holding is what a surveyor would have written by hand.
interface Shot { id: string; from: string; to: string | null; fs: Reading; bs: Reading | null }
const book: Shot[] = [];
let state: SessionState = start('A1', none, { mode: 'leg', backsights: true });
const taken = () => new Set(book.flatMap((s) => [s.from, s.to ?? '']));
let id = 0;
for (const step of traverse({ seed: 5, legs: 8, backsights: true, splays: 2 })) {
  // The surveyor flips to splay mode after each backsight and back to leg mode for the next leg.
  if (step.kind === 'splay') state = withMode(state, 'splay', taken());
  if (step.kind === 'leg') state = withMode(state, 'leg', taken());
  const d = decide(state, r(step.distance, step.azimuth, step.inclination));
  ok(d.plan !== null, `step ${book.length} is accepted`);
  const plan = d.plan as Plan;
  let shot = '';
  if (plan.kind === 'back') {
    book.find((b) => b.id === plan.shot)!.bs = plan.reading;
    shot = plan.shot;
  } else {
    shot = `s${++id}`;
    book.push({ id: shot, from: plan.from, to: plan.kind === 'leg' ? plan.to : null, fs: plan.reading, bs: null });
  }
  state = applied(state, plan, shot, taken());
}
const legs = book.filter((b) => b.to !== null);
const splays = book.filter((b) => b.to === null);
ok(legs.length === 8 && splays.length === 16, 'eight legs and two splays at each of them');
ok(legs.map((l) => `${l.from}-${l.to}`).join(' ') === 'A1-A2 A2-A3 A3-A4 A4-A5 A5-A6 A6-A7 A7-A8 A8-A9', 'the stations run A1 to A9 with no gaps and no repeats');
ok(legs.every((l) => l.bs !== null && backsightWarnings(l.fs, l.bs as Reading).length === 0), 'every leg got its own backsight, and each agrees with its foresight');
ok(splays.every((s, i) => s.from === `A${Math.floor(i / 2) + 2}`), 'splays are taken at the station the surveyor has just reached');

if (bad) process.exit(1);
console.log('survey ok');
