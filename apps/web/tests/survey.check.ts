import { BACK_FIND_DEG, advance, applied, decideEvent, looksLikeBack, standing, backsightWarnings, checkReading, decide, nextStep, skipBack, start, withMode, type Plan, type SessionState } from '../src/lib/survey.ts';
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
const upSplay = checkReading(r(2, 10, 90), true);
ok(upSplay.ok && upSplay.warnings.length === 0, 'but a splay straight up is how a passage is measured: no warning');
ok(decide({ ...start('A1', new Set()), mode: 'splay' }, r(2, 10, -90)).warnings.length === 0, 'in splay mode too');
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

// An instrument that cannot mark a backsight (SAP5, SAP6, BRIC): after a leg with backsights on, a leg that comes back
// along it is its backsight; one that does not is the next leg (the backsight was skipped).
{
  const fore = r(10, 90, 5);
  ok(looksLikeBack(fore, r(10.02, 270.3, -5.2)), 'a leg exactly reversed is a backsight');
  ok(looksLikeBack(fore, r(10.5, 285, -12)), 'and one within 25 degrees of reversed, about as long');
  ok(!looksLikeBack(fore, r(10, 90, 5)), 'a leg going the same way is not');
  ok(!looksLikeBack(fore, r(10, 200, -5)), 'nor one turned 70 degrees');
  ok(!looksLikeBack(fore, r(10, 270, 40)), 'nor one whose clino does not mirror');
  ok(!looksLikeBack(fore, r(14, 270, -5)), 'nor one much longer (14 m against 10 m)');
  ok(looksLikeBack(r(8, 10, 88), r(8.1, 200, -87)), 'on a pitch the compass is not compared');
  ok(BACK_FIND_DEG === 25, 'the tolerance is 25 degrees');
  const awaiting = { ...start('A1', new Set(), { mode: 'auto', backsights: true }), awaiting: { shot: 's1', reading: fore }, last: { shot: 's1', reading: fore } };
  const leg = (rd: Reading) => ({ kind: 'leg' as const, group: 1, reading: rd, shots: 3, mark: 'none' as const, spread: { distance: 0, azimuth: 0, inclination: 0 }, flagged: false });
  const back = decideEvent(awaiting, leg(r(10, 270, -5)), new Set());
  ok(back.decision.plan?.kind === 'back' && back.decision.plan.shot === 's1', 'with no mark on it, a leg coming back is written as the backsight of the leg awaiting one');
  const forward = decideEvent(awaiting, leg(r(6, 30, 2)), new Set());
  ok(forward.decision.plan?.kind === 'leg' && forward.base.from === 'A2', 'a leg going on instead: the backsight was skipped, they have moved on and it is the next leg');
  const marked = decideEvent(awaiting, { ...leg(r(10, 271, -5)), mark: 'backsight' }, new Set());
  ok(marked.decision.plan?.kind === 'back', 'a leg marked backsight on an X1 still is one');
  const nobacks = decideEvent({ ...awaiting, awaiting: null, backsights: false }, leg(r(10, 270, -5)), new Set());
  ok(nobacks.decision.plan?.kind === 'leg', 'with nothing awaited a leg that comes back is just a leg (a switchback)');
}

// LRUD: a shot that measures a wall of the station is a dimension, never a splay
{
  const lrudMode: SessionState = { ...start('A1', new Set(), { mode: 'leg', backsights: false }), mode: 'lrud', last: { shot: 's1', reading: r(5, 100, 0) } };
  const side = (az: number, inc: number) => { const d = decide(lrudMode, r(1.5, az, inc)); return d.plan && d.plan.kind === 'lrud' ? d.plan.side : d.reject ? 'rejected' : 'splay'; };
  ok(side(190, 0) === 'right' && side(10, 0) === 'left', 'LRUD mode: square to the passage is the right or left wall (passage heading 100)');
  ok(side(100, 80) === 'up' && side(280, -70) === 'down', 'LRUD mode: well up or down is the roof or floor, whatever the compass says');
  ok(side(100, 0) === 'rejected' && side(280, 5) === 'rejected', 'LRUD mode: along the passage is not a wall');
  const noHeading = decide({ ...lrudMode, last: null }, r(1.5, 190, 0));
  ok(!noHeading.plan && /left from/.test(noHeading.reject ?? ''), 'LRUD mode: with no leg yet there is no left to speak of, and it says so');
  ok(decide({ ...lrudMode, last: null }, r(2, 0, 88)).plan?.kind === 'lrud', 'LRUD mode: up needs no heading');
  const plan = decide(lrudMode, r(1.5, 190, 0)).plan as Extract<Plan, { kind: 'lrud' }>;
  ok(plan.station === 'A1' && plan.reading.distance === 1.5, 'it is written at the station the surveyor stands at, with the shot length');
  ok(applied(lrudMode, plan, 'x', new Set()) === lrudMode, 'a wall shot moves nobody on');
  const flagged = decide({ ...lrudMode, mode: 'leg' }, { ...r(1.2, 0, 0), lrud: 'left' });
  ok(flagged.plan?.kind === 'lrud' && flagged.plan.side === 'left', 'a reading that says which wall it is needs no LRUD mode');
  const waiting = decide({ ...lrudMode, mode: 'leg', backsights: true, from: 'A1', to: 'A2', awaiting: { shot: 's1', reading: r(5, 100, 0) } }, { ...r(1.2, 0, 0), lrud: 'up' });
  ok(waiting.plan?.kind === 'lrud' && waiting.plan.station === 'A2', 'a wall shot while a backsight is awaited is not the backsight, and belongs to the station stood at');
  const ev = decideEvent({ ...lrudMode, mode: 'auto' }, { kind: 'lrud', reading: { ...r(1.2, 0, 0), lrud: 'down' } }, new Set());
  ok(ev.decision.plan?.kind === 'lrud', 'the X1 path: an LRUD event is a wall dimension');
  const asSplay = decideEvent(lrudMode, { kind: 'splay', reading: r(1.5, 190, 0) }, new Set());
  ok(asSplay.decision.plan?.kind === 'lrud', 'in LRUD mode even what the X1 grouping settled as a splay is a wall');
  ok(/not a splay/.test(nextStep(lrudMode)), 'the instruction says what the shots are');
}

if (bad) process.exit(1);
console.log('survey ok');
