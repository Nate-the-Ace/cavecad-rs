// What a person has chosen for the sheet (paper, way up, scale, the words of the title block and which
// pieces to show). Kept on this machine, like the other view settings: the sheet is a way of printing
// the cave, not part of the cave.

import type { SheetOptions, SheetSetup } from './engine';
import type { Units } from './units';

const KEY = 'cavecad.sheet';

const DEFAULT: SheetSetup = { paper: 8, turned: 2, feetPerInch: 0, name: '', lines: [], title: true, bar: true, north: true };

function saved(): SheetSetup {
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) ?? 'null');
    if (raw && typeof raw === 'object') return { ...DEFAULT, ...raw };
  } catch {
    // nothing saved
  }
  return { ...DEFAULT };
}

/** The plot scales of the units the cave is shown in (1:500 in metric, 1" = 40 ft in imperial), plus the one in use (`keep`, feet per inch) when it is of the other kind. Used by every scale dropdown. */
export function orderedScales(scales: SheetOptions['scales'], units: Units, keep = 0): SheetOptions['scales'] {
  return scales.filter((s) => s.metric === (units === 'metric') || s.feet_per_inch === keep);
}

/** How a plot scale (feet per inch) reads in the units the cave is shown in: `1:1000` or `1" = 80 ft`. */
export function scaleText(feetPerInch: number, units: Units): string {
  if (units === 'metric') return `1:${Math.round(feetPerInch * 12)}`;
  return `1" = ${feetPerInch < 10 ? feetPerInch.toFixed(2) : feetPerInch.toFixed(0)} ft`;
}

// The paper follows the units the cave is shown in (ANSI B, 17 x 11 in, for feet; ISO A3 for metres) until the
// person picks one. Indexes into the engine's paper list.
const CHOSEN_KEY = 'cavecad.sheet.paperChosen';
const PAPER_FOR: Record<Units, number> = { imperial: 1, metric: 8 };
let paperChosen = (() => {
  try {
    return localStorage.getItem(CHOSEN_KEY) === '1';
  } catch {
    return false;
  }
})();

/** Puts the paper of the units on the sheet, unless the person has picked a paper themself. */
export function followUnits(units: Units) {
  if (!paperChosen && sheet.paper !== PAPER_FOR[units]) sheet.paper = PAPER_FOR[units];
}

/** The person picked a paper: it stays, whatever the units. */
export function choosePaper(index: number) {
  paperChosen = true;
  try {
    localStorage.setItem(CHOSEN_KEY, '1');
  } catch {
    // remembered for this visit
  }
  setSheet({ paper: index });
}

/** The sheet's settings (the units' paper by default, the cave's name in the title block). */
export const sheet = $state<SheetSetup>(saved());

/** Changes some of the settings and remembers them. */
export function setSheet(patch: Partial<SheetSetup>) {
  Object.assign(sheet, patch);
  try {
    localStorage.setItem(KEY, JSON.stringify($state.snapshot(sheet)));
  } catch {
    // not remembered
  }
}
