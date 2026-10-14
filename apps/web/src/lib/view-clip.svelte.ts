// Copy, cut and paste of drawn lines between the views (Map, Profile, Cross Section), and the Copy command (pick a base point,
// then click as many times as you like to drop a copy). Each view says what it draws and where the pointer is; this decides the rest.

import { cave, changed, selectIds, selectedIds } from './cave.svelte';
import { engine } from './engine';
import { centreOf, clipFrom, decodeClip, encodeClip, moved, type Clip, type ClipLine, type Drawn } from './clip';
import { lastView } from './select-all';

/** What a view tells the clipboard about itself. */
export interface ClipView {
  /** Every line the view owns, with its id. */
  lines: () => (Drawn & { id: string })[];
  /** Where pasted lines go: the run (profile) or station (section), empty on the map. */
  target: () => string;
  /** Where the pointer last was, in the view's own space, or null. */
  cursor: () => [number, number] | null;
  /** The middle of what the view shows. */
  centre: () => [number, number] | null;
}

const views = new Map<string, ClipView>();

/** Registers a view; returns the call that takes it away. */
export function registerClipView(name: 'map' | 'profile' | 'section', view: ClipView): () => void {
  views.set(name, view);
  return () => {
    views.delete(name);
  };
}

/** The Copy command: which view holds the lines being copied, the base point once picked. */
export const copyOp = $state({ on: false, view: '', base: null as [number, number] | null });

/** The selected lines that a view owns. */
export function selectedIn(name: string): (Drawn & { id: string })[] {
  const ids = new Set(selectedIds());
  return views.get(name)?.lines().filter((l) => ids.has(l.id)) ?? [];
}

/** The view that holds the selection (the one with the most of it), or empty. */
function holderOfSelection(): string {
  let best = '';
  let most = 0;
  for (const name of views.keys()) {
    const n = selectedIn(name).length;
    if (n > most) {
      most = n;
      best = name;
    }
  }
  return best;
}

/** What is selected, as a copy. Null when nothing is. */
export function selectionClip(): Clip | null {
  const name = holderOfSelection();
  return name ? clipFrom(name, selectedIn(name)) : null;
}

/** Puts a copy on the system clipboard from a copy/cut event; false when there is nothing to copy. */
export function copyEvent(e: ClipboardEvent): boolean {
  const clip = selectionClip();
  if (!clip) return false;
  e.preventDefault();
  e.clipboardData?.setData('text/plain', encodeClip(clip));
  return true;
}

/** Deletes the selected lines (after a cut). */
export function deleteSelection() {
  const ids = selectedIds();
  cave.selected = '';
  for (const id of ids) engine.deleteLine(id);
  changed();
}

/** Drops `lines` (already moved into place) into a view as one edit; the new lines are returned (and selected unless `keep`). */
function drop(name: string, lines: ClipLine[], keep = false): boolean {
  const v = views.get(name);
  if (!v) return false;
  const r = engine.pasteLines(name as 'map', v.target(), cave.adjusted, lines);
  if (typeof r === 'string') {
    engine.log('warn', 'paste', r.replace(/^error: /, ''));
    return false;
  }
  changed();
  if (!keep) selectIds(r.ids);
  return true;
}

/** Pasting: a copy waits in the view it will go into, shown faded under the pointer, until a press says where. */
export const pasteOp = $state({ on: false, view: '', clip: null as Clip | null });

/** Starts a paste of a copy into the view last pressed in (else the one it came from). False when there is no view to put it in. */
export function pasteClip(clip: Clip): boolean {
  const name = views.has(lastView()) ? lastView() : views.has(clip.view) ? clip.view : (views.keys().next().value ?? '');
  if (!views.has(name)) return false;
  endCopy();
  pasteOp.clip = clip;
  pasteOp.view = name;
  pasteOp.on = true;
  return true;
}

/** Ends a paste without placing anything. */
export function endPaste() {
  pasteOp.on = false;
  pasteOp.view = '';
  pasteOp.clip = null;
}

/** A press at `p` (the view's own space) while a paste waits in `name`: the copy goes there, its middle at the press. */
export function pastePlace(name: string, p: [number, number]) {
  const clip = pasteOp.clip;
  if (!pasteOp.on || pasteOp.view !== name || !clip) return;
  const c = centreOf(clip);
  endPaste();
  drop(name, moved(clip.lines, p[0] - c[0], p[1] - c[1]));
}

/** What the waiting paste would put down if the pointer were pressed at `cursor`: what to draw faded under it. */
export function pasteGhost(name: string, cursor: [number, number] | null): ClipLine[] | null {
  const clip = pasteOp.clip;
  if (!pasteOp.on || pasteOp.view !== name || !clip || !cursor) return null;
  const c = centreOf(clip);
  return moved(clip.lines, cursor[0] - c[0], cursor[1] - c[1]);
}

/** A paste event: reads our copy off the system clipboard text. */
export function pasteEvent(e: ClipboardEvent): boolean {
  const clip = decodeClip(e.clipboardData?.getData('text/plain'));
  if (!clip) return false;
  e.preventDefault();
  return pasteClip(clip);
}

/** Starts the Copy command on the selection: pick a base point, then click to drop copies. */
export function startCopy() {
  const name = holderOfSelection();
  if (!name) return;
  copyOp.on = true;
  copyOp.view = name;
  copyOp.base = null;
}

/** Ends the Copy command. */
export function endCopy() {
  copyOp.on = false;
  copyOp.view = '';
  copyOp.base = null;
}

/** A press at `p` (the view's own space) while the Copy command runs in `name`: the first sets the base point, each after drops a copy. */
export function copyPress(name: string, p: [number, number]) {
  if (!copyOp.on || copyOp.view !== name) return;
  if (!copyOp.base) {
    copyOp.base = p;
    return;
  }
  const lines = clipFrom(name, selectedIn(name)).lines;
  if (!lines.length) return endCopy();
  drop(name, moved(lines, p[0] - copyOp.base[0], p[1] - copyOp.base[1]), true);
}

/** The copies the Copy command would drop if the pointer were pressed at `cursor`: what to draw faded under it. */
export function copyGhost(name: string, cursor: [number, number] | null): { lines: ClipLine[]; base: [number, number] } | null {
  if (!copyOp.on || copyOp.view !== name || !copyOp.base || !cursor) return null;
  const lines = clipFrom(name, selectedIn(name)).lines;
  return { lines: moved(lines, cursor[0] - copyOp.base[0], cursor[1] - copyOp.base[1]), base: copyOp.base };
}

/** True when a vertex of the selection is the nearest to `p` within `reach` (view units): the base point snaps to it. */
export function snapToSelection(name: string, p: [number, number], reach: number): [number, number] {
  let best = p;
  let bd = reach;
  for (const l of selectedIn(name)) {
    for (const q of l.points) {
      const d = Math.hypot(q[0] - p[0], q[1] - p[1]);
      if (d <= bd) {
        bd = d;
        best = [q[0], q[1]];
      }
    }
  }
  return best;
}
