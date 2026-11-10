// The viewframe picked on a sheet, AutoCAD style: click a frame to pick it (move it, drag its grips to
// resize it, set its scale in the ribbon), double-click inside to work inside it (drag pans its map,
// the wheel zooms it), double-click outside or Escape to go back to the paper. A locked frame keeps its
// scale and view: it can still be moved or resized, but its map cannot be panned or zoomed.
import { changed, setPick, clearPick, cave } from './cave.svelte';
import { engine, type SheetDef, type Viewframe } from './engine';

/** Which frame is picked and whether its map has the focus. */
export const vf = $state({ sheet: '', frame: '', inside: false });

/** Picks a frame (the ribbon shows its tab); empty ids let go. */
export function pickFrame(sheet: string, frame: string) {
  vf.sheet = sheet;
  vf.frame = frame;
  vf.inside = false;
  if (frame) setPick('viewframe', `${sheet}|${frame}`);
  else if (cave.pick.kind === 'viewframe') clearPick();
}

/** The picked sheet and frame as stored, or null. */
export function pickedFrame(): { def: SheetDef; f: Viewframe } | null {
  if (!vf.sheet || !vf.frame) return null;
  const def = engine.sheetsList().find((d) => d.id === vf.sheet);
  const f = def?.frames.find((q) => q.id === vf.frame);
  return def && f ? { def, f } : null;
}

/** Changes the picked frame and stores the sheet as one undoable edit. */
export function editFrame(change: (f: Viewframe, def: SheetDef) => void | false): string {
  const p = pickedFrame();
  if (!p) return 'error: no viewframe picked';
  if (change(p.f, p.def) === false) return 'ok';
  if (p.def.auto) p.def.auto.edited = true; // a hand-changed sheet is kept when its grid is made again
  const r = engine.sheetPut(p.def);
  if (r !== 'ok') engine.log('warn', 'sheet', r);
  changed();
  return r;
}

/** The scale of a frame as people read it: 1:N on paper. */
export function scaleRatio(feetPerInch: number): string {
  return `1:${Math.round(feetPerInch * 12)}`;
}
