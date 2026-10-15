// Object snap for the flat panes (Profile, Cross Section, the sketchpad): one small controller so all of them snap
// the same way, and the same way as the Map. The pane describes itself (its drawn lines, anything else worth snapping to,
// how its plane maps to pixels); this builds the scene for the engine, runs the pick, and draws the feedback. A pane's
// plane is flat (`u` across, `v` up, metres), so tracking angles and distances always make sense in it.

import { acquireHover, chosen, clearPick, commit, cycle, escapePick, overrideName, pickBuiltUp, query, snap, snapping, toggleSnap } from './snap.svelte';
import { drawAperture, drawSnap } from './snap-draw';
import { sceneThings, type SnapLine } from './snap-scene';
import { flatCamera, planFrame, type SnapScene, type SnapThing } from './snap-types';

export interface PaneSnapHooks {
  /** The lines drawn in the pane (points are `[u, v]`). */
  lines: () => SnapLine[];
  /** Anything else a pick can land on: measured points, the station. */
  extra?: () => SnapThing[];
  /** How the plane maps to pixels: `px = tx + u * scale`, `py = ty - v * scale`. */
  tf: () => { scale: number; tx: number; ty: number };
  size: () => { w: number; h: number };
  /** True while the pane is waiting for a point: a tool, Copy, Paste, a leader tip. */
  wants: () => boolean;
  redraw: () => void;
  /** True for a kind whose first point is an insertion point. */
  stamp: (kind: string) => boolean;
  /** True for a kind switched off in the Show menu. */
  hidden?: (kind: string) => boolean;
  /** Opens the one-pick menu at a client position. */
  openMenu: (x: number, y: number) => void;
}

export class PaneSnap {
  private readonly h: PaneSnapHooks;
  /** The pointer on the canvas, pixels. */
  cursor: [number, number] | null = null;
  private hover: ReturnType<typeof setTimeout> | undefined;

  constructor(h: PaneSnapHooks) {
    this.h = h;
  }

  /** What the engine is told about this pane. */
  scene(exclude: string): SnapScene {
    const { w, h } = this.h.size();
    const lines = this.h.lines().filter((l) => l.id !== exclude && !this.h.hidden?.(l.kind));
    return {
      camera: flatCamera(this.h.tf(), w, h),
      things: [...sceneThings(lines, [], { stamp: this.h.stamp, frameOf: () => planFrame(0) }), ...(this.h.extra?.() ?? [])],
      plane: planFrame(0),
    };
  }

  /** True when picks here should be snapped. */
  active(): boolean {
    return this.h.wants() && snapping();
  }

  /**
   * The snap in reach of the pointer at (`x`, `y`) (canvas pixels), as the point to use in the plane, or null when there is
   * none (the pane then uses the raw point). Remembers the result for the overlay and arms the hover that acquires an end.
   */
  pick(x: number, y: number, exclude = ''): { p: [number, number]; snapped: true } | null {
    this.cursor = [x, y];
    if (!this.active()) {
      if (snap.result) snap.result = null;
      return null;
    }
    query(this.scene(exclude), x, y);
    clearTimeout(this.hover);
    this.hover = setTimeout(() => {
      acquireHover();
      if (this.cursor) query(this.scene(exclude), this.cursor[0], this.cursor[1]);
      this.h.redraw();
    }, 300);
    const c = chosen();
    return c ? { p: [c.world[0], c.world[1]], snapped: true } : null;
  }

  /** A pick landed on `p`: the point the tool should use, or null when the pick only set something up (a track point, a base point). */
  land(p: [number, number]): [number, number] | null {
    clearTimeout(this.hover);
    const used = commit([p[0], p[1], 0]);
    return used ? [used[0], used[1]] : null;
  }

  /** The tool finished its pick (a stroke ended): the session is clean again. */
  done() {
    clearTimeout(this.hover);
    clearPick();
  }

  /** Draws the feedback on top of the pane. */
  draw(g: CanvasRenderingContext2D) {
    if (!this.cursor || !this.active()) return;
    const t = this.h.tf();
    const at = drawSnap(g, snap.result, snap.session, {
      cursor: this.cursor,
      aperture: snap.settings.aperture,
      width: this.h.size().w,
      project: (p) => [t.tx + p[0] * t.scale, t.ty - p[1] * t.scale],
      prefs: snap.prefs,
      overrideName: overrideName(),
      tab: snap.tab,
    });
    drawAperture(g, at, snap.settings.aperture);
  }

  /** F3, Tab and Escape. Returns true when the key was used. */
  key(e: KeyboardEvent): boolean {
    if (e.key === 'F3') {
      toggleSnap();
      this.h.redraw();
      return true;
    }
    if (e.key === 'Tab' && this.h.wants() && snap.on) {
      cycle();
      this.h.redraw();
      return true;
    }
    if (e.key === 'Escape' && pickBuiltUp()) {
      escapePick();
      this.h.redraw();
      return true;
    }
    return false;
  }

  /** Shift or Ctrl right click while a point is wanted opens the one-pick menu. Returns true when it did. */
  rightClick(e: MouseEvent): boolean {
    if ((e.shiftKey || e.ctrlKey) && this.h.wants()) {
      this.h.openMenu(e.clientX, e.clientY);
      return true;
    }
    return false;
  }
}
