import { LegAssembler, average, isClose, type LegEvent } from '../src/lib/instrument/legs.ts';
import { shotsFor, type FlagPolicy } from '../src/lib/instrument/x1sim.ts';
import { traverse } from '../src/lib/instrument/simulator.ts';
import type { Reading } from '../src/lib/instrument/types.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const near = (a: number, b: number, e: number) => Math.abs(a - b) <= e;
const r = (distance: number, azimuth: number, inclination: number, extra: Partial<Reading> = {}): Reading => ({ distance, azimuth, inclination, roll: 0, source: 't', ...extra });

// What counts as close.
ok(isClose(r(5, 100, 2), r(5.04, 101.5, 3.5)), 'a few cm and a degree or two: close');
ok(!isClose(r(5, 100, 2), r(5.2, 100, 2)), '20 cm apart is not');
ok(!isClose(r(5, 100, 2), r(5, 104, 2)), 'four degrees of compass is not');
ok(!isClose(r(5, 100, 2), r(5, 100, 6)), 'four degrees of clino is not');
ok(isClose(r(100, 100, 2), r(100.4, 100, 2)), 'on a 100 m shot, 40 cm is within half a percent');
ok(isClose(r(5, 359.5, 0), r(5, 0.8, 0)), 'the compass wraps round north');
ok(isClose(r(8, 10, 88), r(8, 200, 88.5)), 'on a pitch the compass is not compared');
ok(!isClose(r(8, 10, 88), r(8, 200, 80)), 'but the clino still is');

// Averaging: circular mean for the compass.
const north = average([r(5, 359.9, 0), r(5, 0.1, 0), r(5, 0, 0)]).reading;
ok(near(north.azimuth > 180 ? north.azimuth - 360 : north.azimuth, 0, 0.01), 'three shots round north average to north, not to 120');
const avg = average([r(5.00, 100, 10), r(5.02, 101, 11), r(5.01, 99, 12)]);
ok(near(avg.reading.distance, 5.01, 1e-9) && near(avg.reading.azimuth, 100, 0.01) && near(avg.reading.inclination, 11, 1e-9), 'tape and clino by mean, compass by circular mean');
ok(near(avg.spread.distance, 0.02, 1e-9) && near(avg.spread.inclination, 2, 1e-9) && near(avg.spread.azimuth, 2, 0.05), 'the spread is reported');
ok(average([r(3, 10, 0, { mark: 'none' }), r(3, 10, 0, { mark: 'backsight' }), r(3, 10, 0, { mark: 'none' })]).reading.mark === 'backsight', 'a mark on any shot of the leg marks the leg');
ok(average([r(3, 10, 0, { error: null }), r(3, 10, 0, { error: 'accuracy error: absG' })]).reading.error === 'accuracy error: absG', 'an accuracy error on any shot is kept');

// Simple sequences.
{
  const a = new LegAssembler();
  const e1 = a.push(r(5, 100, 0));
  const e2 = a.push(r(5.01, 100.2, 0.1));
  ok(e1.length === 0 && e2.length === 0 && a.waiting === 2, 'one or two close shots wait: they may become a leg');
  const e3 = a.push(r(4.99, 99.9, -0.1));
  ok(e3.length === 1 && e3[0].kind === 'leg' && e3[0].shots === 3, 'the third close shot makes a leg');
  ok(a.waiting === 0, 'nothing is left waiting once it is a leg');
  const e4 = a.push(r(5.0, 100.1, 0));
  ok(e4.length === 1 && e4[0].kind === 'leg-update' && e4[0].shots === 4 && e4[0].group === (e3[0] as { group: number }).group, 'a fourth close shot updates the same leg');
  const e5 = a.push(r(2, 10, 0));
  ok(e5.length === 0 && a.waiting === 1, 'a far shot starts something new and waits');
  const f = a.flush();
  ok(f.length === 1 && f[0].kind === 'splay', 'on a pause, a lone shot is a splay');
}
{
  const a = new LegAssembler();
  a.push(r(5, 100, 0));
  a.push(r(5, 100.1, 0));
  const ev = a.push(r(9, 200, 30));
  ok(ev.length === 2 && ev.every((e) => e.kind === 'splay'), 'two close shots then a far one: two splays');
  ok(a.flush().length === 1, 'and the far one waits');
}
{
  const a = new LegAssembler();
  const ev = a.push(r(5, 100, 0, { leg: true }));
  ok(ev.length === 1 && ev[0].kind === 'leg' && ev[0].shots === 1, 'a shot the device flags as a leg is a leg at once, even alone');
  const up = a.push(r(5.01, 100, 0, { leg: true }));
  ok(up.length === 1 && up[0].kind === 'leg-update' && up[0].shots === 2, 'and the next close shots refine it');
}
{
  const a = new LegAssembler();
  const ev = a.push(r(5, 100, 0, { leg: true, mark: 'backsight' }));
  ok(ev[0].kind === 'leg' && ev[0].mark === 'backsight' && (ev[0] as { flagged: boolean }).flagged, 'a flagged, marked leg says so');
}
ok(new LegAssembler().flush().length === 0, 'flushing nothing gives nothing');
{
  const a = new LegAssembler();
  a.push(r(5, 100, 0));
  a.reset();
  ok(a.flush().length === 0 && a.waiting === 0, 'reset forgets what was waiting');
}

// A whole survey, whichever way the instrument uses its flag: the same legs, backsights and splays come out.
const steps = traverse({ seed: 5, legs: 5, backsights: true, splays: 2 });
for (const policy of ['all', 'third', 'none'] as FlagPolicy[]) {
  const a = new LegAssembler();
  const events: LegEvent[] = [];
  for (const s of shotsFor(steps, policy, 3)) events.push(...a.push({ ...r(s.distance, s.azimuth, s.inclination), leg: s.leg, mark: s.mark }));
  events.push(...a.flush());
  const kinds = events.filter((e) => e.kind !== 'leg-update').map((e) => e.kind);
  ok(kinds.join(' ') === ('leg leg splay splay '.repeat(5)).trim(), `policy ${policy}: leg, backsight leg, two splays, five times over`);
  const legs = events.filter((e): e is Extract<LegEvent, { kind: 'leg' }> => e.kind === 'leg');
  ok(legs.length === 10 && legs.filter((l) => l.group).length === 10 && new Set(legs.map((l) => l.group)).size === 10, `policy ${policy}: ten legs, each with its own group`);
  // The last word on each leg is its final average (the leg event, or its last update).
  const final = new Map<number, Extract<LegEvent, { group: number }>>();
  for (const e of events) if (e.kind !== 'splay') final.set(e.group, e);
  const legSteps = steps.filter((s) => s.kind !== 'splay');
  let good = true;
  [...final.values()].forEach((e, i) => {
    const s = legSteps[i];
    if (!near(e.reading.distance, s.distance, 0.004) || !near(e.reading.azimuth, s.azimuth, 0.3) || !near(e.reading.inclination, s.inclination, 0.3) || e.shots !== 3 || (e.mark === 'backsight') !== (s.kind === 'back')) good = false;
  });
  ok(good && final.size === 10, `policy ${policy}: each leg ends as three shots averaging the surveyed values, and backsights are marked`);
  const splays = events.filter((e) => e.kind === 'splay');
  ok(splays.length === 10, `policy ${policy}: two splays per station`);
  if (policy === 'all') ok(events.filter((e) => e.kind === 'leg-update').length === 20, 'flagged from the first shot: the leg is reported at once, then refined twice');
  else ok(events.filter((e) => e.kind === 'leg-update').length === 0, `policy ${policy}: a leg is reported when the third shot arrives`);
}

// a shot that measures a wall is neither part of a leg nor a splay
{
  const a = new LegAssembler();
  const out = [...a.push(r(5, 100, 0)), ...a.push(r(5.001, 100.1, 0)), ...a.push({ ...r(1.4, 190, 0), lrud: 'right' }), ...a.push(r(5, 100, 0))];
  ok(out.map((e) => e.kind).join() === 'splay,splay,lrud', `a wall shot settles what waited (as splays) and goes on its own: ${out.map((e) => e.kind)}`);
  const b = new LegAssembler();
  const legs = [...b.push(r(5, 100, 0)), ...b.push(r(5.001, 100, 0)), ...b.push(r(5.002, 100, 0)), ...b.push({ ...r(1.4, 190, 0), lrud: 'right' }), ...b.push({ ...r(1.4, 190, 0), lrud: 'left' })];
  ok(legs.map((e) => e.kind).join() === 'leg,lrud,lrud', 'two wall shots as close as two splays never become a leg');
  ok(b.waiting === 0, 'and nothing is left waiting');
}

if (bad) process.exit(1);
console.log('legs ok');
