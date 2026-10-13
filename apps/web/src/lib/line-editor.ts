// Editing drawn linework on a 2D canvas: select a line, pick up one of its fit points and move it,
// double-click to add a fit point, Delete to remove the line. One implementation for every view
// that draws on a canvas (Profile, Cross Section). The view says where its lines are and how
// world and screen relate; this decides what a press, a drag and a key do.
import { distanceToCurve, insertionAt, nearestPoint, refit, type Pt } from './spline.ts';
import { hitsStamp, isStampKind } from './symbols.ts';

/** How near, in pixels, a point is to a line (a symbol counts as its whole body). */
function reach(l: { points: Pt[]; closed?: boolean; kind?: string; text?: string }, toScreen: (p: Pt) => Pt, x: number, y: number): number {
  if (isStampKind(l.kind) && l.points.length >= 2) return hitsStamp(l.kind ?? '', l.text, toScreen(l.points[0]), toScreen(l.points[1]), x, y) ? 0 : Infinity;
  return distanceToCurve(l.points.map(toScreen), x, y, !!l.closed);
}

export interface EditableLine {
  id: string;
  points: Pt[];
  widths: number[] | null;
  /** True when the curve loops back from the last point to the first. */
  closed?: boolean;
  /** What it is (wall, ledge, ...). */
  kind?: string;
  /** Words that go with it. */
  text?: string;
}

export interface Hooks<T extends EditableLine> {
  /** The lines drawn in this view, in drawing order. */
  lines: () => T[];
  toScreen: (p: Pt) => Pt;
  toWorld: (x: number, y: number) => Pt;
  /** The selected line's id (shared across views), and a way to change it. */
  selected: () => string;
  select: (id: string) => void;
  /** Saves new fit points for a line (one undoable edit). */
  commit: (id: string, points: Pt[], widths: number[] | null) => void;
  /** Deletes a line (one undoable edit). */
  remove: (id: string) => void;
  redraw: () => void;
  /** True for a type that is switched off in the Show menu (it cannot be picked, seen or erased). */
  hidden?: (kind: string | undefined) => boolean;
}

/** How close, in pixels, a press must be to a line to select it, and to a fit point to pick it up. */
const LINE_REACH = 7;
const HANDLE_REACH = 10;

export class LineEditor<T extends EditableLine = EditableLine> {
  /** A fit point being moved: the line, which point, and the line's points as they are right now. */
  private drag: { id: string; index: number; points: Pt[]; widths: number[] | null; stamp: boolean } | null = null;

  private readonly h: Hooks<T>;

  constructor(hooks: Hooks<T>) {
    this.h = hooks;
  }

  /** The lines that are switched on. */
  private shown(): T[] {
    const hide = this.h.hidden;
    return hide ? this.h.lines().filter((l) => !hide(l.kind)) : this.h.lines();
  }

  /** The lines to draw: the one being dragged shows its live points. */
  display(): T[] {
    const d = this.drag;
    return this.shown().map((l) => (d && l.id === d.id ? { ...l, points: d.points, widths: d.widths } : l));
  }

  /** Screen positions of the selected line's fit points (empty when nothing is selected here). */
  handles(): Pt[] {
    const l = this.display().find((k) => k.id === this.h.selected());
    return l ? l.points.map(this.h.toScreen) : [];
  }

  private selectedLine(): T | undefined {
    return this.display().find((k) => k.id === this.h.selected());
  }

  /** A press at screen (`x`, `y`). Picks up a fit point of the selected line, or selects the line there. Returns true when a fit point was picked up. */
  press(x: number, y: number): boolean {
    const sel = this.selectedLine();
    if (sel) {
      const i = nearestPoint(sel.points.map(this.h.toScreen), x, y, HANDLE_REACH);
      if (i >= 0) {
        this.drag = { id: sel.id, index: i, points: sel.points.map((p) => [...p] as Pt), widths: sel.widths ? [...sel.widths] : null, stamp: isStampKind(sel.kind) };
        return true;
      }
    }
    const lines = this.shown();
    for (let i = lines.length - 1; i >= 0; i--) {
      if (reach(lines[i], this.h.toScreen, x, y) <= LINE_REACH) {
        this.h.select(lines[i].id);
        this.h.redraw();
        return false;
      }
    }
    if (this.h.selected()) this.h.select('');
    this.h.redraw();
    return false;
  }

  /** The eraser passing screen (`x`, `y`): deletes the topmost line there (one undoable edit). Returns true if it did. */
  eraseAt(x: number, y: number): boolean {
    const lines = this.shown();
    for (let i = lines.length - 1; i >= 0; i--) {
      if (reach(lines[i], this.h.toScreen, x, y) <= LINE_REACH) {
        if (this.h.selected() === lines[i].id) this.h.select('');
        this.h.remove(lines[i].id);
        this.h.redraw();
        return true;
      }
    }
    return false;
  }

  /** Fits a drawn line again at another curve detail (1 to 10), as one edit. Returns true if it changed. */
  refitLine(id: string, detail: number): boolean {
    const line = this.shown().find((k) => k.id === id);
    if (!line || isStampKind(line.kind) || line.points.length < 3) return false;
    const f = refit(line.points, line.widths, !!line.closed, detail);
    if (f.pts.length === line.points.length && f.pts.every((p, i) => p[0] === line.points[i][0] && p[1] === line.points[i][1])) return false;
    this.h.commit(id, f.pts, f.widths);
    return true;
  }

  /** Lets go of a picked-up fit point without saving (a second finger arrived: it was a gesture). */
  cancel(): void {
    this.drag = null;
    this.h.redraw();
  }

  /** Moves the picked-up fit point; returns true while one is being moved. */
  move(x: number, y: number): boolean {
    if (!this.drag) return false;
    const to = this.h.toWorld(x, y);
    if (this.drag.stamp && this.drag.index === 0) {
      // A symbol's insertion point carries the whole symbol, its size and turn handle with it.
      const dx = to[0] - this.drag.points[0][0];
      const dy = to[1] - this.drag.points[0][1];
      this.drag.points = this.drag.points.map((q) => [q[0] + dx, q[1] + dy] as Pt);
    } else this.drag.points[this.drag.index] = to;
    this.h.redraw();
    return true;
  }

  /** Lets go: saves the moved line. Returns true if a fit point was being moved. */
  release(): boolean {
    const d = this.drag;
    if (!d) return false;
    this.drag = null;
    this.h.commit(d.id, d.points, d.widths);
    return true;
  }

  /** A double-click on the selected line adds a fit point there. Returns true if it did. */
  addPoint(x: number, y: number): boolean {
    const sel = this.selectedLine();
    if (!sel) return false;
    const screen = sel.points.map(this.h.toScreen);
    if (distanceToCurve(screen, x, y, !!sel.closed) > LINE_REACH + 3) return false;
    const { index, at } = insertionAt(screen, x, y, !!sel.closed);
    const points = [...sel.points];
    points.splice(index, 0, this.h.toWorld(at[0], at[1]));
    const widths = sel.widths ? [...sel.widths] : null;
    if (widths) widths.splice(index, 0, (widths[index - 1] + widths[index % widths.length]) / 2);
    this.h.commit(sel.id, points, widths);
    return true;
  }

  /** Delete or Backspace removes the selected line; Escape lets go of it. Returns true if the key was used. */
  key(e: KeyboardEvent): boolean {
    const sel = this.selectedLine();
    if (!sel) return false;
    if (e.key === 'Delete' || e.key === 'Backspace') {
      this.h.remove(sel.id);
      this.h.select('');
      return true;
    }
    if (e.key === 'Escape') {
      this.h.select('');
      this.h.redraw();
      return true;
    }
    return false;
  }
}

/** How close, in pixels, a press must be to the end of an open line to carry that line on. */
export const PICKUP_REACH = 12;

/** The open line of kind `kind` with an end near screen (`x`, `y`): which line, which end, where on screen. */
export function endpointNear<T extends EditableLine>(lines: T[], kind: string, toScreen: (p: Pt) => Pt, x: number, y: number, reach = PICKUP_REACH): { line: T; atStart: boolean; at: Pt } | null {
  let best: { line: T; atStart: boolean; at: Pt } | null = null;
  let bestD = reach;
  for (const line of lines) {
    if (line.closed || line.kind !== kind || line.points.length < 2) continue;
    for (const atStart of [false, true]) {
      const at = toScreen(atStart ? line.points[0] : line.points[line.points.length - 1]);
      const d = Math.hypot(at[0] - x, at[1] - y);
      if (d <= bestD) {
        bestD = d;
        best = { line, atStart, at };
      }
    }
  }
  return best;
}

/**
 * A new stroke (as fit points) carried on from an end of an existing open line: the line's own end
 * point is where the new stroke joins, so there is never a gap. Drawn away from the start, the
 * new points go in front, in the order they will run along the line.
 */
export function joinFit(line: EditableLine, fit: { pts: Pt[]; widths: number[] | null }, atStart: boolean): { pts: Pt[]; widths: number[] | null } {
  const added = fit.pts.slice(1);
  const addedW = fit.widths ? fit.widths.slice(1) : null;
  const useWidths = line.widths !== null || fit.widths !== null;
  const own = line.widths ?? line.points.map(() => 1);
  const ownNew = addedW ?? added.map(() => 1);
  if (atStart) {
    return { pts: [...added.reverse(), ...line.points], widths: useWidths ? [...ownNew.reverse(), ...own] : null };
  }
  return { pts: [...line.points, ...added], widths: useWidths ? [...own, ...ownNew] : null };
}
