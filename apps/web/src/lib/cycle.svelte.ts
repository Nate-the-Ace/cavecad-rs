// Selection cycling, as in AutoCAD: when a click lands on several things stacked on top of each other
// (a symbol over a wall over a fill, two levels of passage over each other in plan), the topmost one is
// selected as always, and a small list of everything under the click opens beside the pointer so any
// other one can be chosen instead. One list for the whole app; each view says what was under its click.

import { cave, option, setOption } from './cave.svelte';

/** One thing under the click. */
export interface Candidate {
  /** Unique within the list (e.g. `line:<id>`, `station:<id>`). */
  key: string;
  /** What it is, as a person reads it ("Wall", "Station A7"). */
  label: string;
  /** A little more, to tell alike ones apart (words on it, its trip, ...). */
  detail?: string;
  /** Selects it. `final` is false while the pointer or the arrow keys only pass over it (no panels open, nothing logged). */
  choose: (final: boolean) => void;
}

export const cycle = $state({
  open: false,
  x: 0,
  y: 0,
  items: [] as Candidate[],
  /** The row lit (and selected) right now. */
  index: 0,
  /** The row that was chosen (the top one until another is clicked): where passing over the rows returns to. */
  chosen: 0,
  /** `cave.version` when the list opened: an edit closes it (a row could name a thing that is gone). */
  version: 0,
});

/** Whether a click on stacked things lists them (on unless switched off; remembered on this machine). */
export const cyclingOn = (): boolean => option('selectionCycling', true);

export function setCycling(on: boolean) {
  setOption('selectionCycling', on);
  if (!on) closeCycle();
}

/**
 * A click at screen (`x`, `y`) landed on `items`, topmost first, and the view has already selected the first. Opens the
 * list when there is more than one (and cycling is on); otherwise closes any list still open.
 */
export function offerStack(x: number, y: number, items: Candidate[]) {
  const unique = items.filter((it, i) => items.findIndex((o) => o.key === it.key) === i);
  if (!cyclingOn() || unique.length < 2) {
    closeCycle();
    return;
  }
  cycle.x = x;
  cycle.y = y;
  cycle.items = unique;
  cycle.index = 0;
  cycle.chosen = 0;
  cycle.version = cave.version;
  cycle.open = true;
}

/** Lights row `i` and selects its thing (`final` when it was clicked or Entered). */
export function lightRow(i: number, final = false) {
  const it = cycle.items[i];
  if (!cycle.open || !it) return;
  cycle.index = i;
  it.choose(final);
  if (final) cycle.chosen = i;
}

/** The pointer left the list without clicking: the chosen row is selected again. */
export function backToChosen() {
  if (cycle.open && cycle.index !== cycle.chosen) lightRow(cycle.chosen);
}

/** Commits the lit row and closes the list. */
export function acceptRow() {
  if (cycle.open) lightRow(cycle.index, true);
  closeCycle();
}

export function closeCycle() {
  cycle.open = false;
  cycle.items = [];
}
