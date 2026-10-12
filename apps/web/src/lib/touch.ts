// Fingers on a canvas. One finger draws (while a drawing tool runs) or pans; two fingers pan and
// pinch-zoom together, and a second finger landing mid-stroke cancels that stroke (it was the
// start of a gesture, not a line). This tracks the fingers and turns two of them into a gesture.

export interface Gesture {
  /** How far the middle of the fingers moved since the last move. */
  dx: number;
  dy: number;
  /** How much the fingers spread or closed since the last move (1 = no change). */
  scale: number;
  /** Where the middle of the fingers is. */
  cx: number;
  cy: number;
}

export class Touches {
  private pts = new Map<number, [number, number]>();
  private last: { cx: number; cy: number; d: number } | null = null;

  /** Fingers down now. */
  get count(): number {
    return this.pts.size;
  }

  private summary(): { cx: number; cy: number; d: number } | null {
    const [a, b] = [...this.pts.values()];
    if (!a || !b) return null;
    return { cx: (a[0] + b[0]) / 2, cy: (a[1] + b[1]) / 2, d: Math.max(1, Math.hypot(a[0] - b[0], a[1] - b[1])) };
  }

  /** A finger landed; returns how many are down. */
  down(e: { pointerId: number; clientX: number; clientY: number }): number {
    this.pts.set(e.pointerId, [e.clientX, e.clientY]);
    this.last = this.summary();
    return this.pts.size;
  }

  /** A finger moved; with two or more down, returns how the gesture changed. */
  move(e: { pointerId: number; clientX: number; clientY: number }): Gesture | null {
    if (!this.pts.has(e.pointerId)) return null;
    this.pts.set(e.pointerId, [e.clientX, e.clientY]);
    const now = this.summary();
    const before = this.last;
    this.last = now;
    if (!now || !before) return null;
    return { dx: now.cx - before.cx, dy: now.cy - before.cy, scale: now.d / before.d, cx: now.cx, cy: now.cy };
  }

  /** A finger lifted (or the touch was cancelled); returns how many are still down. */
  up(e: { pointerId: number }): number {
    this.pts.delete(e.pointerId);
    this.last = this.summary();
    return this.pts.size;
  }
}
