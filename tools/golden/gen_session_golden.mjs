// Writes what the web app's own Survey session rules (apps/web/src/lib/survey.ts) decide for a spread of states
// and readings, for the Rust copy (crates/cave-session) to match exactly:
//   node --experimental-strip-types tools/golden/gen_session_golden.mjs <out.json>
// Run it against the commit that still has apps/web/src/lib/survey.ts (survey-sketch before the port).
import { writeFileSync } from 'node:fs';
import { LegAssembler } from '../../apps/web/src/lib/instrument/legs.ts';
import { traverse } from '../../apps/web/src/lib/instrument/simulator.ts';
import { shotsFor } from '../../apps/web/src/lib/instrument/devsim.ts';
import { advance, applied, backsightWarnings, checkReading, decide, decideEvent, looksLikeBack, nextStep, sideOf, skipBack, start, standing, withMode } from '../../apps/web/src/lib/survey.ts';

const ID = (n) => `${'0'.repeat(25)}${'0123456789ABCDEFGHJKMNPQRSTVWXYZ'[n]}`;
const R = (distance, azimuth, inclination, extra = {}) => ({ distance, azimuth, inclination, roll: 0, source: 't', ...extra });

const readings = [
  R(5, 100, 2), R(5.5, 280, -2), R(5.5, 281, 20), R(12, 10, 88), R(12, 190, -88), R(0.02, 10, 0), R(0, 10, 0), R(-1, 10, 0),
  R(700, 10, 0), R(5, 10, 91), R(5, 10, -91), R(2, 10, 60), R(2, 10, -60), R(2, 100, 0), R(2, 190, 0), R(2, 280, 0), R(2, 10, 0),
  R(2, 100, 0, { lrud: 'left' }), R(2, 100, 0, { lrud: 'up' }), R(8, 359, 3, { error: 'accuracy error: absG' }),
  R(5, 100, 2, { mark: 'backsight' }), R(5.4, 285, -4), R(5.2, 280, -9), R(5.2, 250, -2), R(9.9, 280, -2),
  // the edges of the wall aim (45 degrees) and of the backsight find (25 degrees)
  R(2, 54, 0), R(2, 56, 0), R(2, 144, 0), R(2, 146, 0), R(2, 234, 0), R(2, 236, 0), R(2, 324, 0), R(2, 326, 0), R(2, 10, 44), R(2, 10, 46), R(2, 10, -44), R(2, 10, -46),
  R(5, 120, -2), R(5, 124, -2), R(5, 126, -2), R(5, 100, -26), R(5, 100, -24),
];

const aw = (shot, reading) => ({ shot: ID(shot), reading });
const fore = R(5, 100, 2);
const states = [];
for (const mode of ['auto', 'leg', 'splay', 'lrud']) {
  for (const backsights of [false, true]) {
    states.push({ mode, backsights, from: 'A1', to: 'A2', awaiting: null, last: null });
    states.push({ mode, backsights, from: 'A1', to: 'A2', awaiting: aw(1, fore), last: aw(1, fore) });
    states.push({ mode, backsights, from: 'A2', to: 'A3', awaiting: null, last: aw(2, R(6, 100, 3)) });
    states.push({ mode, backsights, from: '', to: '', awaiting: null, last: null });
    states.push({ mode, backsights, from: 'A1', to: '', awaiting: null, last: null });
    states.push({ mode, backsights, from: 'A1', to: 'A1', awaiting: null, last: null });
    states.push({ mode, backsights, from: ' A1 ', to: ' A2 ', awaiting: null, last: null });
  }
}
const takens = [[], ['A1', 'A2', 'A3'], ['A2', 'A3', 'A4', 'A5']];

const out = {};
out.check = readings.flatMap((r) => [false, true].map((splay) => ({ reading: r, splay, result: checkReading(r, splay) })));
out.back = readings.flatMap((a) => readings.slice(0, 8).map((b) => ({ fore: a, back: b, warnings: backsightWarnings(a, b), looks: looksLikeBack(a, b) })));
out.side = readings.flatMap((r) => [null, 0, 10, 100, 190, 280, 359].map((heading) => ({ reading: r, heading, side: sideOf(r, heading) })));
out.decide = states.flatMap((state) => readings.map((reading) => ({ state, reading, result: decide(state, reading) })));
out.moves = states.flatMap((state) => takens.map((taken) => ({
  state, taken,
  advance: advance(state, new Set(taken)),
  skip: skipBack(state, new Set(taken)),
  modes: ['auto', 'leg', 'splay', 'lrud'].map((m) => withMode(state, m, new Set(taken))),
  step: nextStep(state),
  standing: standing(state),
  applied: ['leg', 'splay', 'lrud', 'back'].map((kind) => {
    const plan = kind === 'leg' ? { kind, from: 'A1', to: 'A2', reading: fore } : kind === 'splay' ? { kind, from: 'A1', reading: fore } : kind === 'lrud' ? { kind, station: 'A1', side: 'left', reading: fore } : { kind, shot: ID(1), reading: fore };
    return { plan, state: applied(state, plan, ID(7), new Set(taken)) };
  }),
})));
out.start = [['A1', [], undefined], ['A9', ['A10'], { mode: 'auto', backsights: true }], ['B', [], undefined], ['C07', ['C08'], { mode: 'splay', backsights: false }]].map(([from, taken, keep]) => ({ from, taken, keep: keep ?? null, state: start(from, new Set(taken), keep) }));

// Events from the grouping, on survey data: every kind, with marks, errors and spreads.
const events = [];
for (const policy of ['all', 'third', 'none']) {
  const steps = traverse({ seed: 5, legs: 3, backsights: true, splays: 4, startSplays: true, lrud: true });
  const a = new LegAssembler();
  for (const s of shotsFor(steps, policy)) events.push(...a.push(R(s.distance, s.azimuth, s.inclination, { leg: s.leg, mark: s.mark, ...(s.lrud ? { lrud: s.lrud } : {}) })));
  events.push(...a.flush());
}
const spread = (d, a, i) => ({ distance: d, azimuth: a, inclination: i });
const sampled = events.filter((e) => e.kind !== 'leg-update').filter((_, i) => i % 3 === 0);
events.length = 0;
events.push(...sampled,
  { kind: 'leg', group: 90, reading: R(5, 100, 2, { leg: true, mark: 'none', error: 'accuracy error: absG' }), shots: 3, mark: 'none', spread: spread(0.1, 3, 2.5), flagged: false },
  { kind: 'leg', group: 91, reading: R(5, 280, -2, { leg: true, mark: 'backsight' }), shots: 3, mark: 'backsight', spread: spread(0.01, 0.5, 0.5), flagged: true },
  { kind: 'splay', reading: R(3, 10, 70, { mark: 'none' }) },
  { kind: 'splay', reading: R(3, 10, 91) },
  { kind: 'lrud', reading: R(3, 10, 0, { lrud: 'right' }) },
  { kind: 'lrud', reading: R(3, 10, 0) },
  { kind: 'leg', group: 92, reading: R(5, 100, 90.5, { leg: true }), shots: 3, mark: 'none', spread: spread(0, 0, 0), flagged: false },
);
out.events = [];
const eventStates = states.filter((_, i) => i % 7 === 0 || i % 7 === 1 || i % 7 === 3);
for (const state of eventStates) for (const taken of takens.slice(0, 2)) for (const ev of events) out.events.push({ state, taken, ev, result: decideEvent(state, ev, new Set(taken)) });

writeFileSync(process.argv[2], JSON.stringify(out));
console.log('session golden:', Object.entries(out).map(([k, v]) => `${k}:${v.length}`).join(' '));
