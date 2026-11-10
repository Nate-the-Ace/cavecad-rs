// One eraser drag, the same in every drawing view (Map, Profile, Cross Section and the sketchpad panes). The view
// turns the pointer into its own metres and hands the points here; on release this works out what the drag did to
// the lines the view is showing (lib/erase.ts) and applies it as one undoable edit. Nothing changes until the
// pointer lifts, so a drag can be watched (the circle and its trail) before it counts.

import { eraseLines, type EraseLine, type EraseResult, type Pt } from './erase';
import { engine } from './engine';
import { cave, option } from './cave.svelte';

export type { EraseLine, EraseResult, Pt };

/**
 * The eraser's radius in drawing metres. Its size (`eraserSize`) is the circle's width in millimetres on the printed
 * page, and the drawing is printed at 1:N (`engine.drawingScale()` is N), so a millimetre of paper is N/1000 metres.
 */
export function eraserRadius(): number {
  return (option('eraserSize', 4) * engine.drawingScale()) / 1000 / 2;
}

/** The eraser's radius on screen, for a view drawn at `pxPerMetre` pixels to the metre. */
export function eraserRadiusPx(pxPerMetre: number): number {
  return eraserRadius() * pxPerMetre;
}

export class EraserDrag {
  /** Where the eraser has been during this drag, in the view's metres (empty when no drag is running). */
  path: Pt[] = [];
  /** The radius this drag erases with (metres), fixed when it starts so a changed setting never changes it halfway. */
  radius = 0;

  get active(): boolean {
    return this.path.length > 0;
  }

  /** The eraser touches down at `p` (view metres). */
  start(p: Pt): void {
    this.radius = eraserRadius();
    this.path = [[p[0], p[1]]];
  }

  /** The eraser moves to `p`. Points closer than a quarter of the radius to the last add nothing the cut needs. Returns true when the path grew. */
  move(p: Pt): boolean {
    if (!this.active) return false;
    const last = this.path[this.path.length - 1];
    if (Math.hypot(p[0] - last[0], p[1] - last[1]) < this.radius / 4) return false;
    this.path.push([p[0], p[1]]);
    return true;
  }

  /** Drops the drag without erasing anything (a second finger arrived, the system took the pointer). */
  cancel(): void {
    this.path = [];
  }

  /** Ends the drag: what it does to `lines` (the lines the view shows, in its metres), or null when it touches nothing. */
  take(lines: EraseLine[]): EraseResult | null {
    const path = this.path;
    this.path = [];
    if (!path.length) return null;
    const r = eraseLines(lines, path, this.radius);
    return r.remove.length || r.edits.length || r.adds.length ? r : null;
  }

  /**
   * Ends the drag and applies it to the cave as one undoable edit in `view` (`target` is the run on a profile band, the
   * station on a cross section, '' on the map). Returns true when anything changed; a drag that touched nothing asks
   * nothing of the engine, so it leaves no empty step in the undo list.
   */
  finish(lines: EraseLine[], view: 'map' | 'profile' | 'section', target: string): boolean {
    const r = this.take(lines);
    if (!r) return false;
    const done = engine.eraseLines(view, target, cave.adjusted, r);
    if (!done.ok) engine.log('warn', 'draw', `The eraser could not erase: ${done.message}`);
    return done.ok;
  }
}

/**
 * Paints the eraser on a view's 2D canvas: the trail it has swept this drag (`trail`, screen pixels) as a faint band of
 * its width, and the circle of its size at `at` (the pointer). Either may be missing.
 */
export function paintEraser(g: CanvasRenderingContext2D, trail: Pt[], at: Pt | null, radiusPx: number, color: string): void {
  const r = Math.max(1, radiusPx);
  g.save();
  g.setLineDash([]);
  if (trail.length) {
    g.globalAlpha = 0.18;
    g.strokeStyle = color;
    g.fillStyle = color;
    g.lineCap = 'round';
    g.lineJoin = 'round';
    g.lineWidth = 2 * r;
    if (trail.length === 1) {
      g.beginPath();
      g.arc(trail[0][0], trail[0][1], r, 0, Math.PI * 2);
      g.fill();
    } else {
      g.beginPath();
      g.moveTo(trail[0][0], trail[0][1]);
      for (const p of trail.slice(1)) g.lineTo(p[0], p[1]);
      g.stroke();
    }
  }
  if (at) {
    g.globalAlpha = 1;
    g.lineWidth = 1.5;
    g.strokeStyle = color;
    g.beginPath();
    g.arc(at[0], at[1], r, 0, Math.PI * 2);
    g.stroke();
  }
  g.restore();
}
