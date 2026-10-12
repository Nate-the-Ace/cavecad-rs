// Where a station's cross-section sketches sit in the Map and Profile, and how they are hit-tested.
// A section is a small drawing in its own plane ([across, up] metres from its station); a view
// draws it at true size beside the station, joined to it by a dashed leader, and the person drags
// it to where they want it. The offset they leave it at is saved with the drawing.

import type { Look } from './ink';
import { smooth } from './spline.ts';

export type Sketches = { points: [number, number][]; widths: number[] | null }[];

/** `[minU, minV, maxU, maxV]` around the strokes (zeros when empty). */
export function sketchBounds(sketches: Sketches): [number, number, number, number] {
  let b: [number, number, number, number] | null = null;
  for (const k of sketches) {
    for (const [u, v] of k.points) {
      b = b ? [Math.min(b[0], u), Math.min(b[1], v), Math.max(b[2], u), Math.max(b[3], v)] : [u, v, u, v];
    }
  }
  return b ?? [0, 0, 0, 0];
}

/** Where an unplaced section sits in the map: just east of its station, clear of the strokes. */
export function defaultMapOffset(sketches: Sketches): [number, number, number] {
  const b = sketchBounds(sketches);
  return [Math.max(0, -b[0]) + 1.5, 0, 0];
}

/** Where an unplaced section sits on the band: just above its station, clear of the strokes. */
export function defaultBandOffset(sketches: Sketches): [number, number] {
  const b = sketchBounds(sketches);
  return [0, Math.max(0, -b[1]) + 2];
}

/** True when the screen point is on a section's strokes' box (grown by `pad` px). `px` is pixels per metre. */
export function hits(sketches: Sketches, anchor: [number, number], px: number, x: number, y: number, pad = 8): boolean {
  const b = sketchBounds(sketches);
  return x >= anchor[0] + b[0] * px - pad && x <= anchor[0] + b[2] * px + pad && y >= anchor[1] - b[3] * px - pad && y <= anchor[1] - b[1] * px + pad;
}

/** Draws strokes as smooth curves through their fit points, in the look of their kind (width scaled
 *  by pen weight), `to` mapping section metres to pixels. The stroke with id `selected` gets a halo. */
export function drawStrokes(
  g: CanvasRenderingContext2D,
  sketches: (Sketches[number] & { kind: string; id?: string })[],
  to: (u: number, v: number) => [number, number],
  look: (kind: string) => Look,
  scale = 1,
  selected = '',
): void {
  g.lineCap = 'round';
  g.lineJoin = 'round';
  for (const k of sketches) {
    const l = look(k.kind);
    const curve = smooth(
      k.points.map((p) => to(p[0], p[1])),
      k.widths,
    );
    const trace = (extra: number, color: string, alpha: number, dash: number[]) => {
      g.strokeStyle = color;
      g.globalAlpha = alpha;
      g.setLineDash(dash);
      for (let i = 1; i < curve.pts.length; i++) {
        g.lineWidth = l.weight * scale * (curve.widths ? (curve.widths[i - 1] + curve.widths[i]) / 2 : 1) + extra;
        g.beginPath();
        g.moveTo(curve.pts[i - 1][0], curve.pts[i - 1][1]);
        g.lineTo(curve.pts[i][0], curve.pts[i][1]);
        g.stroke();
      }
    };
    if (selected && k.id === selected) trace(7, l.color, 0.25, []);
    trace(0, l.color, l.alpha, l.dash);
  }
  g.setLineDash([]);
  g.globalAlpha = 1;
}

/** Draws the fit points of a selected stroke as handles that can be picked up. */
export function drawHandles(g: CanvasRenderingContext2D, pts: [number, number][], color: string, bg: string): void {
  g.lineWidth = 1.5;
  for (const p of pts) {
    g.fillStyle = bg;
    g.strokeStyle = color;
    g.beginPath();
    g.rect(p[0] - 4.5, p[1] - 4.5, 9, 9);
    g.fill();
    g.stroke();
  }
}

/** The smooth curves of a section's strokes on screen (what is actually drawn). */
export function glyphCurves(sketches: Sketches, to: (u: number, v: number) => [number, number]): [number, number][][] {
  return sketches.map((k) => smooth(k.points.map((p) => to(p[0], p[1])), null).pts);
}

/**
 * Where a leader from the station should stop: on the outer shell of the drawing. It runs from the
 * station toward the middle of the sketch and ends at the first stroke it meets, so it never
 * crosses into the drawing; if it would miss every stroke (a gap), it ends at the stroke nearest
 * the station. Also returns the drawing's middle and a spot just under it for the label.
 */
export function leaderEnd(from: [number, number], curves: [number, number][][]): { end: [number, number]; centre: [number, number]; below: [number, number] } {
  const all = curves.flat();
  if (all.length === 0) return { end: from, centre: from, below: from };
  const xs = all.map((p) => p[0]);
  const ys = all.map((p) => p[1]);
  const centre: [number, number] = [(Math.min(...xs) + Math.max(...xs)) / 2, (Math.min(...ys) + Math.max(...ys)) / 2];
  const below: [number, number] = [centre[0], Math.max(...ys) + 13];
  const dx = centre[0] - from[0];
  const dy = centre[1] - from[1];
  let bestT = Infinity;
  let nearest: [number, number] = all[0];
  let nearestD = Infinity;
  for (const c of curves) {
    for (let i = 1; i < c.length; i++) {
      const [a, b] = [c[i - 1], c[i]];
      const ex = b[0] - a[0];
      const ey = b[1] - a[1];
      // where the leader (from + t * d) crosses this piece of stroke (a + u * e)
      const det = dx * ey - dy * ex;
      if (Math.abs(det) > 1e-9) {
        const t = ((a[0] - from[0]) * ey - (a[1] - from[1]) * ex) / det;
        const u = ((a[0] - from[0]) * dy - (a[1] - from[1]) * dx) / det;
        if (t >= 0 && t <= 1 && u >= 0 && u <= 1) bestT = Math.min(bestT, t);
      }
      // the point of this piece nearest the station, in case the leader misses every stroke
      const len2 = ex * ex + ey * ey;
      const u = len2 < 1e-12 ? 0 : Math.max(0, Math.min(1, ((from[0] - a[0]) * ex + (from[1] - a[1]) * ey) / len2));
      const q: [number, number] = [a[0] + u * ex, a[1] + u * ey];
      const d = Math.hypot(q[0] - from[0], q[1] - from[1]);
      if (d < nearestD) {
        nearestD = d;
        nearest = q;
      }
    }
  }
  const end: [number, number] = Number.isFinite(bestT) ? [from[0] + dx * bestT, from[1] + dy * bestT] : nearest;
  return { end, centre, below };
}
