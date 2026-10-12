// Editing drawn linework on a 2D canvas: select a line, pick up one of its fit points and move it,
// double-click to add a fit point, Delete to remove the line. One implementation for every view
// that draws on a canvas (Profile, Cross Section). The view says where its lines are and how
// world and screen relate; this decides what a press, a drag and a key do.
import { distanceToCurve, insertionAt, nearestPoint, type Pt } from './spline.ts';

export interface EditableLine {
  id: string;
  points: Pt[];
  widths: number[] | null;
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
}

/** How close, in pixels, a press must be to a line to select it, and to a fit point to pick it up. */
const LINE_REACH = 7;
const HANDLE_REACH = 10;

export class LineEditor<T extends EditableLine = EditableLine> {
  /** A fit point being moved: the line, which point, and the line's points as they are right now. */
  private drag: { id: string; index: number; points: Pt[]; widths: number[] | null } | null = null;

  private readonly h: Hooks<T>;

  constructor(hooks: Hooks<T>) {
    this.h = hooks;
  }

  /** The lines to draw: the one being dragged shows its live points. */
  display(): T[] {
    const d = this.drag;
    return this.h.lines().map((l) => (d && l.id === d.id ? { ...l, points: d.points, widths: d.widths } : l));
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
        this.drag = { id: sel.id, index: i, points: sel.points.map((p) => [...p] as Pt), widths: sel.widths ? [...sel.widths] : null };
        return true;
      }
    }
    const lines = this.h.lines();
    for (let i = lines.length - 1; i >= 0; i--) {
      if (distanceToCurve(lines[i].points.map(this.h.toScreen), x, y) <= LINE_REACH) {
        this.h.select(lines[i].id);
        this.h.redraw();
        return false;
      }
    }
    if (this.h.selected()) this.h.select('');
    this.h.redraw();
    return false;
  }

  /** Moves the picked-up fit point; returns true while one is being moved. */
  move(x: number, y: number): boolean {
    if (!this.drag) return false;
    this.drag.points[this.drag.index] = this.h.toWorld(x, y);
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
    if (distanceToCurve(screen, x, y) > LINE_REACH + 3) return false;
    const { index, at } = insertionAt(screen, x, y);
    const points = [...sel.points];
    points.splice(index, 0, this.h.toWorld(at[0], at[1]));
    const widths = sel.widths ? [...sel.widths] : null;
    if (widths) widths.splice(index, 0, (widths[index - 1] + widths[index]) / 2);
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
