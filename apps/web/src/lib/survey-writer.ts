// Writing a decided `Plan` into the notebook, and taking it back out. Kept apart from the reactive store
// so it can be tested against a stand-in notebook (tests/surveywriter.check.ts): the engine is only
// reached through `NotebookPort`, which `survey.svelte.ts` fills in with the real notebook calls.
//
// The notebook's add-shot call answers only "ok", so the id of the shot it made is found by comparing
// the page before and after. That also holds when the page lays the new shot out somewhere other than
// the end (a splay is shown under its own station).

import type { NotebookPage } from './notebook.ts';
import type { Reading } from './instrument/types.ts';
import type { Plan } from './survey.ts';

export type CellName = 'distance' | 'az_fs' | 'az_bs' | 'inc_fs' | 'inc_bs';

/** The notebook calls the survey session needs. Each edit answers `ok` or `error: ...`. */
export interface NotebookPort {
  page(trip: string): NotebookPage | null;
  addShot(trip: string, from: string, to: string, splay: boolean): string;
  setReading(shot: string, which: CellName, value: number | null): string;
  removeShot(shot: string): string;
}

export type Written = { ok: true; shot: string; label: string } | { ok: false; error: string };

const bad = (r: string) => r.startsWith('error');
const clean = (r: string) => r.replace(/^error: /, '');
const shotIds = (page: NotebookPage | null) => new Set((page?.rows ?? []).flatMap((r) => (r.shot ? [r.shot] : [])));

/** The cells a reading fills: all three for a foresight, compass and clino only for a backsight (the page has one tape cell). */
function cells(r: Reading, which: 'fs' | 'bs'): [CellName, number][] {
  const out: [CellName, number][] = [];
  if (which === 'fs') out.push(['distance', r.distance]);
  out.push([which === 'fs' ? 'az_fs' : 'az_bs', r.azimuth], [which === 'fs' ? 'inc_fs' : 'inc_bs', r.inclination]);
  return out;
}

/** Writes the readings of `r` into the cells of `shot`. Returns `ok` or the first error. */
function fill(port: NotebookPort, shot: string, r: Reading, which: 'fs' | 'bs'): string {
  for (const [cell, value] of cells(r, which)) {
    const result = port.setReading(shot, cell, value);
    if (bad(result)) return result;
  }
  return 'ok';
}

/** Carries out a plan. A shot that cannot be completed is removed again: a half-written shot is never left on the page. */
export function write(port: NotebookPort, trip: string, plan: Plan, standingBack?: { from: string; to: string }): Written {
  if (plan.kind === 'back') {
    const result = fill(port, plan.shot, plan.reading, 'bs');
    if (bad(result)) {
      // Clear whatever part of the backsight went in, so a failed one leaves the foresight as it was.
      port.setReading(plan.shot, 'az_bs', null);
      port.setReading(plan.shot, 'inc_bs', null);
      return { ok: false, error: clean(result) };
    }
    const where = standingBack ? `backsight ${standingBack.to.trim()} to ${standingBack.from.trim()}` : 'backsight';
    return { ok: true, shot: plan.shot, label: where };
  }
  const known = shotIds(port.page(trip));
  const splay = plan.kind === 'splay';
  const added = port.addShot(trip, plan.from, splay ? '' : plan.to, splay);
  if (bad(added)) return { ok: false, error: clean(added) };
  const shot = [...shotIds(port.page(trip))].find((id) => !known.has(id));
  if (!shot) return { ok: false, error: 'The shot was added but could not be found on the page.' };
  const result = fill(port, shot, plan.reading, 'fs');
  if (bad(result)) {
    port.removeShot(shot);
    return { ok: false, error: clean(result) };
  }
  return { ok: true, shot, label: splay ? `splay at ${plan.from}` : `${plan.from} to ${plan.to}` };
}

/** Replaces the readings of a shot already written (a leg that gained a close shot, so its average moved). Returns `ok` or an error. */
export function rewrite(port: NotebookPort, shot: string, r: Reading, which: 'fs' | 'bs'): string {
  return fill(port, shot, r, which);
}

/** Takes a written plan back out: the shot is removed, or a backsight's cells are cleared. Returns `ok` or an error. */
export function undo(port: NotebookPort, kind: Plan['kind'], shot: string): string {
  if (kind !== 'back') return port.removeShot(shot);
  for (const cell of ['az_bs', 'inc_bs'] as const) {
    const result = port.setReading(shot, cell, null);
    if (bad(result)) return result;
  }
  return 'ok';
}
