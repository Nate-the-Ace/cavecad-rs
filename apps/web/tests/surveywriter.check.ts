import { undo, write } from '../src/lib/survey-writer.ts';
import { fakeNotebook } from './fake-notebook.ts';
import { applied, decide, start, withMode, type Plan, type SessionState } from '../src/lib/survey.ts';
import { traverse } from '../src/lib/instrument/simulator.ts';
import type { Reading } from '../src/lib/instrument/types.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const r = (distance: number, azimuth: number, inclination: number): Reading => ({ distance, azimuth, inclination, roll: 0, source: 'test' });

const leg = (from: string, to: string, reading: Reading): Plan => ({ kind: 'leg', from, to, reading });

// A leg: three cells filled, the id found by comparing pages.
{
  const nb = fakeNotebook();
  const w = write(nb.port, 't', leg('A1', 'A2', r(4.2, 100, -5)));
  ok(w.ok && w.label === 'A1 to A2' && nb.rows.length === 1, 'a leg is written and named');
  const row = nb.rows[0];
  ok(row.from === 'A1' && row.to === 'A2' && row.cells.distance === 4.2 && row.cells.az_fs === 100 && row.cells.inc_fs === -5 && row.cells.az_bs === undefined, 'the foresight cells hold the reading and the backsight cells are untouched');
}

// A splay goes to the front of this page, and a later leg is added after it: each id must still be the new one.
{
  const nb = fakeNotebook();
  write(nb.port, 't', leg('A1', 'A2', r(4, 10, 0)));
  const sp = write(nb.port, 't', { kind: 'splay', from: 'A2', reading: r(1.5, 80, 3) });
  const l2 = write(nb.port, 't', leg('A2', 'A3', r(5, 20, 1)));
  ok(sp.ok && l2.ok && sp.shot !== l2.shot, 'the page lays a splay out first, and the writer still finds each new shot');
  ok(sp.ok && nb.rows.find((x) => x.id === sp.shot)?.cells.distance === 1.5 && nb.rows.find((x) => x.id === sp.shot)?.to === null, 'the splay holds its own reading');
  ok(l2.ok && nb.rows.find((x) => x.id === l2.shot)?.cells.distance === 5, 'and the leg holds its own');
  ok(sp.ok && sp.label === 'splay at A2', 'a splay is named for its station');
}

// A backsight completes the foresight's shot: compass and clino only, the tape stays the foresight's.
{
  const nb = fakeNotebook();
  const l = write(nb.port, 't', leg('A1', 'A2', r(4.2, 100, -5)));
  const b = write(nb.port, 't', { kind: 'back', shot: l.ok ? l.shot : '', reading: r(4.3, 280.5, 5.2) }, { from: 'A1', to: 'A2' });
  const row = nb.rows[0];
  ok(b.ok && b.label === 'backsight A2 to A1', 'a backsight is named from the new station back to the old');
  ok(row.cells.az_bs === 280.5 && row.cells.inc_bs === 5.2 && row.cells.distance === 4.2 && row.cells.az_fs === 100, 'it fills the backsight cells and leaves the foresight and the tape alone');
}

// Refusals leave nothing half done.
{
  const nb = fakeNotebook((_s, which) => which === 'inc_fs');
  const w = write(nb.port, 't', leg('A1', 'A2', r(4, 10, 0)));
  ok(!w.ok && w.error === 'that value is out of range', 'the engine\'s reason is passed on, without its prefix');
  ok(nb.rows.length === 0, 'a leg that could not be completed is taken off the page again');
}
{
  const nb = fakeNotebook();
  const w = write(nb.port, 't', leg('A1', 'A1', r(4, 10, 0)));
  ok(!w.ok && nb.rows.length === 0, 'a shot the notebook refuses is not written');
}
{
  let refuse = false;
  const nb = fakeNotebook((_s, which) => refuse && which === 'inc_bs');
  const l = write(nb.port, 't', leg('A1', 'A2', r(4.2, 100, -5)));
  refuse = true;
  const b = write(nb.port, 't', { kind: 'back', shot: l.ok ? l.shot : '', reading: r(4.2, 280, 5) });
  ok(!b.ok && nb.rows[0].cells.az_bs === null && nb.rows[0].cells.inc_bs === undefined, 'a backsight that fails half way clears the half that went in');
  ok(nb.rows[0].cells.az_fs === 100 && nb.rows[0].cells.distance === 4.2, 'and the foresight is as it was');
}

// Undo.
{
  const nb = fakeNotebook();
  const l = write(nb.port, 't', leg('A1', 'A2', r(4.2, 100, -5)));
  const shot = l.ok ? l.shot : '';
  write(nb.port, 't', { kind: 'back', shot, reading: r(4.2, 280, 5) });
  ok(undo(nb.port, 'back', shot) === 'ok' && nb.rows[0].cells.az_bs === null && nb.rows[0].cells.inc_bs === null && nb.rows[0].cells.az_fs === 100, 'undoing a backsight clears only the backsight');
  ok(undo(nb.port, 'leg', shot) === 'ok' && nb.rows.length === 0, 'undoing a leg removes it');
  ok(undo(nb.port, 'leg', shot).startsWith('error'), 'undoing what is already gone says so');
}

// The whole session through the real rules and the stand-in notebook.
{
  const nb = fakeNotebook();
  let state: SessionState = start('A1', new Set(), { mode: 'leg', backsights: true });
  const taken = () => new Set(nb.rows.flatMap((x) => [x.from, x.to ?? '']));
  for (const step of traverse({ seed: 9, legs: 6, backsights: true, splays: 2 })) {
    state = withMode(state, step.kind === 'splay' ? 'splay' : 'leg', taken());
    const d = decide(state, r(step.distance, step.azimuth, step.inclination));
    ok(d.plan !== null, 'every simulated reading is accepted');
    const w = write(nb.port, 't', d.plan as Plan, state);
    ok(w.ok, 'and written');
    if (!w.ok) break;
    state = applied(state, d.plan as Plan, w.shot, taken());
  }
  const legs = nb.rows.filter((x) => x.to !== null);
  ok(legs.length === 6 && nb.rows.length === 6 + 12, 'six legs and two splays at each');
  ok(legs.map((x) => `${x.from}-${x.to}`).join(' ') === 'A1-A2 A2-A3 A3-A4 A4-A5 A5-A6 A6-A7', 'the legs run A1 to A7');
  ok(legs.every((x) => typeof x.cells.az_bs === 'number' && typeof x.cells.inc_bs === 'number'), 'every leg has its backsight');
  ok(nb.rows.filter((x) => x.to === null).every((x) => typeof x.cells.distance === 'number' && x.cells.az_bs === undefined), 'splays have a foresight reading and no backsight');
}

// writing and taking back a wall dimension
{
  const nb = fakeNotebook();
  const plan: Plan = { kind: 'lrud', station: 'A2', side: 'left', reading: r(1.75, 10, 0) };
  const w = write(nb.port, 't', plan);
  ok(w.ok && w.label === 'left wall at A2' && nb.walls['A2:left'] === 1.75 && nb.rows.length === 0, 'a wall dimension goes to the station, not onto the page as a shot');
  ok(w.ok && undo(nb.port, 'lrud', w.shot, 't') === 'ok' && nb.walls['A2:left'] === null, 'taking it back clears the dimension');
}

if (bad) process.exit(1);
console.log('surveywriter ok');
