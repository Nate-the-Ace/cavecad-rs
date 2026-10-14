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

/** The plot scales in the order people want them: metric ones first when the cave is shown in metric, imperial first otherwise. Used by every scale dropdown. */
export function orderedScales(scales: SheetOptions['scales'], units: Units): SheetOptions['scales'] {
  return [...scales].sort((a, b) => (units === 'metric' ? Number(b.metric) - Number(a.metric) : Number(a.metric) - Number(b.metric)));
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
