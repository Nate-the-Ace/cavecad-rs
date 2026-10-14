// Editing drawn linework on a 2D canvas: select a line, pick up one of its fit points and move it,
// double-click to add a fit point, Delete to remove the line. One implementation for every view
// that draws on a canvas (Profile, Cross Section). The view says where its lines are and how
// world and screen relate; this decides what a press, a drag and a key do.
import { distanceToCurve, distanceToPolyline, insertionAt, insideCurve, nearestPoint, refit, type Pt } from './spline.ts';
import { isFillKind } from './fills.ts';
import { hitsLabelNote, hitsStamp, isStampKind, leaderLook } from './symbols.ts';
import { arrowAt, attachOf, nearestVertex, resolveLeaders, withArrow, withoutArrow } from './leader-edit.ts';
import { withAttach } from './line-style-text.ts';

/** How near, in pixels, a point is to a line (a symbol counts as its whole body). */
function reach(l: { points: Pt[]; closed?: boolean; kind?: string; text?: string; style?: string }, toScreen: (p: Pt) => Pt, x: number, y: number, paperPx?: number, ornament?: OrnamentOf): number {
  if (isStampKind(l.kind) && l.points.length >= 2) return hitsStamp(l.kind ?? '', l.text, toScreen(l.points[0]), toScreen(l.points[1]), x, y, 4, leaderLook(l.style, paperPx), l.points.slice(2).map(toScreen)) ? 0 : Infinity;
  let d = distanceToCurve(l.points.map(toScreen), x, y, !!l.closed);
  // a shaped line (ledge ticks, pit hachures, flowstone, slope, ...) is picked by any part of its marks, not only by the line itself
  if (ornament && d > 2) for (const line of ornament(l) ?? []) d = Math.min(d, distanceToPolyline(line.map(toScreen), x, y));
  return d;
}

/** The marks that dress a shaped line, as polylines in the view's own coordinates (none for a plain line). */
export type OrnamentOf = (l: { points: Pt[]; closed?: boolean; kind?: string; id?: string }) => Pt[][];

/**
 * The topmost line at a point: a line, symbol or area's edge within reach first (topmost wins), then any area the point is
 * inside, so the hatch of a fill can be clicked as well as its outline, while a click on a wall or symbol that lies over a
 * fill still gets the wall or symbol. Returns the index, or -1.
 */
export function topLineAt<T extends { points: Pt[]; closed?: boolean; kind?: string; text?: string; style?: string }>(lines: T[], toScreen: (p: Pt) => Pt, x: number, y: number, reachPx: number, paperPx?: number, ornament?: OrnamentOf): number {
  for (let i = lines.length - 1; i >= 0; i--) if (reach(lines[i], toScreen, x, y, paperPx, ornament) <= reachPx) return i;
  for (let i = lines.length - 1; i >= 0; i--) if (isFillKind(lines[i].kind) && lines[i].points.length >= 3 && insideCurve(lines[i].points.map(toScreen), x, y)) return i;
  return -1;
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
  /** How this one line differs from its type's look (and a leader's arrow, line, frame and size). */
  style?: string;
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
  /** How many pixels a millimetre of paper is here (a leader's text is sized on paper), when the view knows. */
  paperPx?: () => number;
  /** The marks that dress a shaped line, so it can be picked by any part of them. */
  ornament?: OrnamentOf;
  /** A leader waiting for its next press to add or remove an arrow (`''` for none); `arrowDone` says it was used. */
  arrowOp?: (id: string) => '' | 'add' | 'remove';
  arrowDone?: () => void;
  /** Saves a leader's moved points together with its style (where its arrows are fastened) as one undoable edit. */
  commitLeader?: (id: string, points: Pt[], style: string) => void;
}

/** How close, in pixels, a press must be to a line to select it, and to a fit point to pick it up. */
const LINE_REACH = 7;
const HANDLE_REACH = 10;
/** How close, in pixels, a dragged arrow tip must come to another thing's point to fasten to it. */
const SNAP_REACH = 10;

export class LineEditor<T extends EditableLine = EditableLine> {
  /** A fit point being moved: the line, which point, and the line's points as they are right now. */
  private drag: { id: string; index: number; points: Pt[]; widths: number[] | null; stamp: boolean; leader: boolean } | null = null;

  private readonly h: Hooks<T>;

  constructor(hooks: Hooks<T>) {
    this.h = hooks;
  }

  /** The lines that are switched on. */
  private shown(): T[] {
    const hide = this.h.hidden;
    return resolveLeaders(hide ? this.h.lines().filter((l) => !hide(l.kind)) : this.h.lines());
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
    const op = sel?.kind === 'callout' ? this.h.arrowOp?.(sel.id) : '';
    if (sel && op) {
      const screen = sel.points.map(this.h.toScreen);
      const spot = nearestVertex(this.shown(), sel.id, this.h.toScreen, x, y, SNAP_REACH) ?? this.h.toWorld(x, y);
      const next = op === 'add' ? withArrow(sel.points, spot) : withoutArrow(sel.points, arrowAt(screen, x, y, HANDLE_REACH + 6));
      if (next) this.saveLeader(sel.id, next, sel.style);
      this.h.arrowDone?.();
      this.h.redraw();
      return true;
    }
    if (sel) {
      const i = nearestPoint(sel.points.map(this.h.toScreen), x, y, HANDLE_REACH);
      if (i >= 0) {
        this.drag = { id: sel.id, index: i, points: sel.points.map((p) => [...p] as Pt), widths: sel.widths ? [...sel.widths] : null, stamp: isStampKind(sel.kind), leader: sel.kind === 'callout' };
        return true;
      }
    }
    // a symbol's label note is picked up where it sits, selected or not
    for (const k of [...this.shown()].reverse()) {
      if (!k.text || k.points.length < 3 || !hitsLabelNote(k.kind, k.text, this.h.toScreen(k.points[0]) as [number, number], this.h.toScreen(k.points[1]) as [number, number], this.h.toScreen(k.points[2]) as [number, number], x, y)) continue;
      this.h.select(k.id);
      this.drag = { id: k.id, index: 2, points: k.points.map((p) => [...p] as Pt), widths: k.widths ? [...k.widths] : null, stamp: true, leader: false };
      this.h.redraw();
      return true;
    }
    const lines = this.shown();
    const hit = topLineAt(lines, this.h.toScreen, x, y, LINE_REACH, this.h.paperPx?.(), this.h.ornament);
    if (hit >= 0) {
      this.h.select(lines[hit].id);
      this.h.redraw();
      return false;
    }
    if (this.h.selected()) this.h.select('');
    this.h.redraw();
    return false;
  }

  /** The eraser passing screen (`x`, `y`): deletes the topmost line there (one undoable edit). Returns true if it did. */
  eraseAt(x: number, y: number): boolean {
    const lines = this.shown();
    const hit = topLineAt(lines, this.h.toScreen, x, y, LINE_REACH, this.h.paperPx?.(), this.h.ornament);
    if (hit < 0) return false;
    if (this.h.selected() === lines[hit].id) this.h.select('');
    this.h.remove(lines[hit].id);
    this.h.redraw();
    return true;
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

  /** Saves a leader's points and what its arrows are now fastened to, as one edit. */
  private saveLeader(id: string, points: Pt[], style: string | undefined) {
    if (!this.h.commitLeader) return this.h.commit(id, points, null);
    this.h.commitLeader(id, points, withAttach(style, attachOf(points, this.shown(), id)));
  }

  /** Moves the picked-up fit point; returns true while one is being moved. */
  move(x: number, y: number): boolean {
    if (!this.drag) return false;
    const to = this.h.toWorld(x, y);
    if (this.drag.stamp && this.drag.index === 0 && !this.drag.leader) {
      // A symbol's insertion point carries the whole symbol, its size and turn handle with it.
      const dx = to[0] - this.drag.points[0][0];
      const dy = to[1] - this.drag.points[0][1];
      this.drag.points = this.drag.points.map((q) => [q[0] + dx, q[1] + dy] as Pt);
    } else if (this.drag.leader && this.drag.index !== 1) {
      // a leader's arrow snaps to another thing's point, and is then fastened to it
      this.drag.points[this.drag.index] = nearestVertex(this.shown(), this.drag.id, this.h.toScreen, x, y, SNAP_REACH) ?? to;
    } else this.drag.points[this.drag.index] = to;
    this.h.redraw();
    return true;
  }

  /** Lets go: saves the moved line. Returns true if a fit point was being moved. */
  release(): boolean {
    const d = this.drag;
    if (!d) return false;
    this.drag = null;
    if (d.leader && this.h.commitLeader) this.saveLeader(d.id, d.points, this.selectedLine()?.style);
    else this.h.commit(d.id, d.points, d.widths);
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
