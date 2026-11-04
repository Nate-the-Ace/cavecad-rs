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

/** The sheet's settings (A3 by default, the cave's name in the title block). */
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
