// What a person has chosen for the sheet (paper, way up, scale, the words of the title block and which
// pieces to show). Kept on this machine, like the other view settings: the sheet is a way of printing
// the cave, not part of the cave.

import type { SheetSetup } from './engine';

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
