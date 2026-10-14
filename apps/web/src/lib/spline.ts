// Linework is drawn as smooth curves through a few fit points. A freehand stroke is reduced to its
// fit points when the pen lifts (so there is something to select and adjust), and every view draws
// the same curve through them: a centripetal Catmull-Rom spline, which never overshoots or loops
// on uneven spacing.

export type Pt = [number, number];

/** Perpendicular distance from `p` to the segment `a`-`b`. */
function segmentDistance(p: Pt, a: Pt, b: Pt): number {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const len2 = dx * dx + dy * dy;
  const t = len2 < 1e-18 ? 0 : Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2));
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}

/**
 * Reduces a freehand stroke to fit points: the fewest points that keep the curve within `tol` of
 * the drawing (Ramer-Douglas-Peucker). Pen weights go with the points that stay.
 */
export function simplify(pts: Pt[], widths: number[] | null, tol: number): { pts: Pt[]; widths: number[] | null } {
  if (pts.length <= 2) return { pts, widths };
  const keep = new Array<boolean>(pts.length).fill(false);
  keep[0] = keep[pts.length - 1] = true;
  const stack: [number, number][] = [[0, pts.length - 1]];
  while (stack.length) {
    const [lo, hi] = stack.pop()!;
    let worst = -1;
    let at = -1;
    for (let i = lo + 1; i < hi; i++) {
      const d = segmentDistance(pts[i], pts[lo], pts[hi]);
      if (d > worst) {
        worst = d;
        at = i;
      }
    }
    if (worst > tol && at > 0) {
      keep[at] = true;
      stack.push([lo, at], [at, hi]);
    }
  }
  const idx = keep.flatMap((k, i) => (k ? [i] : []));
  return { pts: idx.map((i) => pts[i]), widths: widths ? idx.map((i) => widths[i]) : null };
}

/**
 * Samples of the curve through `pts` (and the pen weight at each sample), `per` samples per span.
 * A closed curve also runs from the last point back to the first, smoothly, with no corner there.
 */
export function smooth(pts: Pt[], widths: number[] | null = null, per = 10, closed = false): { pts: Pt[]; widths: number[] | null } {
  const n = pts.length;
  if (n < 3) return { pts, widths };
  const at = (i: number): Pt => pts[((i % n) + n) % n];
  const ext: Pt[] = closed
    ? [at(-1), ...pts, at(n), at(n + 1)]
    : [[2 * pts[0][0] - pts[1][0], 2 * pts[0][1] - pts[1][1]], ...pts, [2 * pts[n - 1][0] - pts[n - 2][0], 2 * pts[n - 1][1] - pts[n - 2][1]]];
  const spans = closed ? n : n - 1;
  const out: Pt[] = [pts[0]];
  const w: number[] = widths ? [widths[0]] : [];
  const knot = (a: Pt, b: Pt) => Math.max(Math.pow(Math.hypot(b[0] - a[0], b[1] - a[1]), 0.5), 1e-6);
  for (let i = 0; i < spans; i++) {
    const [p0, p1, p2, p3] = [ext[i], ext[i + 1], ext[i + 2], ext[i + 3]];
    const t0 = 0;
    const t1 = t0 + knot(p0, p1);
    const t2 = t1 + knot(p1, p2);
    const t3 = t2 + knot(p2, p3);
    for (let s = 1; s <= per; s++) {
      const t = t1 + ((t2 - t1) * s) / per;
      const mix = (a: Pt, b: Pt, ta: number, tb: number): Pt => [a[0] * ((tb - t) / (tb - ta)) + b[0] * ((t - ta) / (tb - ta)), a[1] * ((tb - t) / (tb - ta)) + b[1] * ((t - ta) / (tb - ta))];
      const a1 = mix(p0, p1, t0, t1);
      const a2 = mix(p1, p2, t1, t2);
      const a3 = mix(p2, p3, t2, t3);
      const b1 = mix(a1, a2, t0, t2);
      const b2 = mix(a2, a3, t1, t3);
      out.push(mix(b1, b2, t1, t2));
      if (widths) w.push(widths[i] + (((widths[(i + 1) % n] ?? widths[i]) - widths[i]) * s) / per);
    }
  }
  return { pts: out, widths: widths ? w : null };
}

/** The fit point index of `pts` within `radius` of (`x`, `y`) (the nearest), or -1. */
export function nearestPoint(pts: Pt[], x: number, y: number, radius: number): number {
  let best = -1;
  let bestD = radius;
  pts.forEach((p, i) => {
    const d = Math.hypot(p[0] - x, p[1] - y);
    if (d <= bestD) {
      bestD = d;
      best = i;
    }
  });
  return best;
}

/** True when (`x`, `y`) is inside the closed curve through `pts` (the smooth curve, not just the fit points). */
export function insideCurve(pts: Pt[], x: number, y: number): boolean {
  if (pts.length < 3) return false;
  const c = smooth(pts, null, 8, true).pts;
  let inside = false;
  for (let i = 0, j = c.length - 1; i < c.length; j = i++) {
    const [xi, yi] = c[i];
    const [xj, yj] = c[j];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

/** Distance from (`x`, `y`) to the curve through `pts`. */
export function distanceToCurve(pts: Pt[], x: number, y: number, closed = false): number {
  const c = smooth(pts, null, 10, closed).pts;
  let d = Infinity;
  for (let i = 1; i < c.length; i++) d = Math.min(d, segmentDistance([x, y], c[i - 1], c[i]));
  return c.length === 1 ? Math.hypot(c[0][0] - x, c[0][1] - y) : d;
}

/** Where a new fit point belongs to land on the span of `pts` nearest (`x`, `y`): its index and the point on the span. */
export function insertionAt(pts: Pt[], x: number, y: number, closed = false): { index: number; at: Pt } {
  let best = { index: 1, at: pts[0], d: Infinity };
  // a closed curve also has the span from the last point back to the first
  for (let i = 1; i < pts.length + (closed ? 1 : 0); i++) {
    const [a, b] = [pts[i - 1], pts[i % pts.length]];
    const dx = b[0] - a[0];
    const dy = b[1] - a[1];
    const len2 = dx * dx + dy * dy;
    const t = len2 < 1e-18 ? 0 : Math.max(0, Math.min(1, ((x - a[0]) * dx + (y - a[1]) * dy) / len2));
    const at: Pt = [a[0] + t * dx, a[1] + t * dy];
    const d = Math.hypot(x - at[0], y - at[1]);
    if (d < best.d) best = { index: i, at, d };
  }
  return { index: best.index, at: best.at };
}

/**
 * A freehand stroke as fit points: reduced within `tol`, and for a closed curve the last point
 * (which the hand left beside the first) is dropped when it sits within reach of it.
 */
export function fitStroke(pts: Pt[], widths: number[] | null, tol: number, closed: boolean): { pts: Pt[]; widths: number[] | null } {
  const f = simplify(pts, widths, tol);
  if (closed && f.pts.length > 3) {
    const [a, b] = [f.pts[0], f.pts[f.pts.length - 1]];
    if (Math.hypot(a[0] - b[0], a[1] - b[1]) < tol * 3) return { pts: f.pts.slice(0, -1), widths: f.widths ? f.widths.slice(0, -1) : null };
  }
  return f;
}

/** How far a re-fit may stray from the curve it is fitted to, as a fraction of the curve's length, for a curve detail from 1 (few points) to 10 (follows the curve closely). */
export function detailFraction(detail: number): number {
  const d = Math.min(10, Math.max(1, detail));
  return 0.03 * Math.pow(0.035, (d - 1) / 9); // 1 -> 3% of the length, 5 -> 0.67%, 10 -> 0.1%
}

/**
 * Fits a line that is already drawn again at another curve detail. The raw hand movement is long
 * gone, so the curve that is on screen is what is fitted: sampled finely, then reduced to the fewest
 * points that keep it within `detailFraction(detail)` of its length. A lower detail gives fewer
 * points (easier to edit), a higher one more (closer to the curve); the ends and pen weights stay.
 */
export function refit(pts: Pt[], widths: number[] | null, closed: boolean, detail: number): { pts: Pt[]; widths: number[] | null } {
  if (pts.length < 3) return { pts, widths };
  const dense = smooth(pts, widths, 10, closed);
  let length = 0;
  for (let i = 1; i < dense.pts.length; i++) length += Math.hypot(dense.pts[i][0] - dense.pts[i - 1][0], dense.pts[i][1] - dense.pts[i - 1][1]);
  const tol = length * detailFraction(detail);
  const f = simplify(dense.pts, dense.widths, tol);
  if (!closed) return f;
  // the last sample of a closed curve is its first point again
  const out = { pts: f.pts.slice(0, -1), widths: f.widths ? f.widths.slice(0, -1) : null };
  return out.pts.length >= 3 ? out : { pts, widths };
}
