// The eraser's geometry: given the lines a view shows and the path an eraser drag took, work out what
// is removed, what is trimmed and what is split off. Pure numbers, no engine and no canvas, so the same
// answer comes out whatever view (map, profile, cross section) the drag was made in.
//
// The caller turns the result into one undoable edit: `remove` deletes whole features, `edits` reshapes
// a feature that keeps its id, `adds` creates a new feature copying the style of `from`.

import { isFillKind } from './fills.ts';
import { isStampKind } from './symbols.ts';

export type Pt = [number, number];
export type EraseLine = { id: string; kind: string; points: Pt[]; widths?: number[] | null; closed?: boolean };
export type EraseResult = {
  remove: string[];
  edits: { id: string; points: Pt[]; widths: number[] | null }[];
  adds: { from: string; points: Pt[]; widths: number[] | null }[];
};

// Kind tests are the two one-line predicates from lib/fills.ts (isFillKind) and lib/symbols.ts
// (isStampKind), imported above so they cannot drift apart from what the rest of the app means.

/** Distance from point p to the segment a-b (a == b is a dot). */
function pointSeg(p: Pt, a: Pt, b: Pt): number {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const l2 = dx * dx + dy * dy;
  const t = l2 === 0 ? 0 : Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2));
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}

/** Distance from point p to a polyline; a one-point polyline is a dot. */
function pointPath(p: Pt, path: Pt[]): number {
  if (path.length === 1) return Math.hypot(p[0] - path[0][0], p[1] - path[0][1]);
  let best = Infinity;
  for (let i = 0; i + 1 < path.length; i++) best = Math.min(best, pointSeg(p, path[i], path[i + 1]));
  return best;
}

function cross(o: Pt, a: Pt, b: Pt): number {
  return (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0]);
}

/** Smallest distance between segments a-b and c-d (0 when they cross). */
function segSeg(a: Pt, b: Pt, c: Pt, d: Pt): number {
  const d1 = cross(a, b, c);
  const d2 = cross(a, b, d);
  const d3 = cross(c, d, a);
  const d4 = cross(c, d, b);
  if (((d1 > 0 && d2 < 0) || (d1 < 0 && d2 > 0)) && ((d3 > 0 && d4 < 0) || (d3 < 0 && d4 > 0))) return 0;
  return Math.min(pointSeg(a, c, d), pointSeg(b, c, d), pointSeg(c, a, b), pointSeg(d, a, b));
}

/** Even-odd point-in-polygon (the polygon closes itself). */
function inside(p: Pt, poly: Pt[]): boolean {
  let c = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i];
    const [xj, yj] = poly[j];
    if (yi > p[1] !== yj > p[1] && p[0] < ((xj - xi) * (p[1] - yi)) / (yj - yi) + xi) c = !c;
  }
  return c;
}

/** A fill goes whole when the path is inside it or touches its outline. */
function fillHit(points: Pt[], path: Pt[], radius: number): boolean {
  if (points.length === 0) return false;
  if (points.length >= 3 && path.some((p) => inside(p, points))) return true;
  if (points.length === 1) return pointPath(points[0], path) <= radius;
  const n = points.length;
  for (let i = 0; i < n; i++) {
    const a = points[i];
    const b = points[(i + 1) % n]; // the closing side counts
    for (let k = 0; k < path.length; k++) {
      const c = path[k];
      const d = path[k + 1] ?? c;
      if (segSeg(a, b, c, d) <= radius) return true;
    }
  }
  return false;
}

type Run = { pts: Pt[]; ws: number[] | null };

/** Cuts one open polyline (with optional per-point widths) against the eraser. Returns the kept runs and
 *  whether anything at all was erased. */
function cutOpen(points: Pt[], widths: number[] | null, path: Pt[], radius: number): { runs: Run[]; touched: boolean } {
  const erased = (p: Pt) => pointPath(p, path) <= radius;
  const step = radius / 4;
  const runs: Run[] = [];
  let touched = false;
  let cur: Run | null = null;
  const push = (p: Pt, w: number | null) => {
    cur!.pts.push(p);
    if (cur!.ws && w !== null) cur!.ws.push(w);
  };
  const open = (p: Pt, w: number | null) => {
    cur = { pts: [p], ws: widths ? [w as number] : null };
    runs.push(cur);
  };

  if (erased(points[0])) touched = true;
  else open(points[0], widths ? widths[0] : null);

  for (let i = 0; i + 1 < points.length; i++) {
    const a = points[i];
    const b = points[i + 1];
    const wa = widths ? widths[i] : 0;
    const wb = widths ? widths[i + 1] : 0;
    const at = (t: number): Pt => [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    const wAt = (t: number) => (widths ? wa + (wb - wa) * t : null);
    // Sample finely enough that a gap narrower than the circle cannot slip between samples.
    const n = Math.max(1, Math.ceil(Math.hypot(b[0] - a[0], b[1] - a[1]) / step));
    let tPrev = 0;
    let prevErased = cur === null;
    for (let k = 1; k <= n; k++) {
      const t = k / n;
      const e = erased(at(t));
      if (e) touched = true;
      if (e !== prevErased) {
        // Somewhere in (tPrev, t) the state flips: bisect to put the boundary exactly on the circle.
        let lo = tPrev;
        let hi = t;
        for (let s = 0; s < 40; s++) {
          const mid = (lo + hi) / 2;
          if (erased(at(mid)) === prevErased) lo = mid;
          else hi = mid;
        }
        // The kept side of the boundary is lo when we were kept, hi when we were erased.
        const tb = prevErased ? hi : lo;
        if (prevErased) open(at(tb), wAt(tb));
        else {
          push(at(tb), wAt(tb));
          cur = null;
        }
        prevErased = e;
      }
      tPrev = t;
    }
    // Reached the end vertex still kept: the run carries on through it.
    if (cur !== null) push(b, widths ? wb : null);
  }
  return { runs, touched };
}

const runLength = (r: Run) => r.pts.slice(1).reduce((s, p, i) => s + Math.hypot(p[0] - r.pts[i][0], p[1] - r.pts[i][1]), 0);

export function eraseLines(lines: EraseLine[], path: Pt[], radius: number): EraseResult {
  const out: EraseResult = { remove: [], edits: [], adds: [] };
  if (path.length === 0) return out;

  for (const line of lines) {
    const pts = line.points;
    if (pts.length === 0) continue;

    if (isFillKind(line.kind)) {
      if (fillHit(pts, path, radius)) out.remove.push(line.id);
      continue;
    }
    if (isStampKind(line.kind)) {
      if (pts.some((p) => pointPath(p, path) <= radius)) out.remove.push(line.id);
      continue;
    }

    const hasW = !!line.widths && line.widths.length === pts.length;
    let ring = pts;
    let ws: number[] | null = hasW ? line.widths! : null;
    const closed = !!line.closed && pts.length > 2;
    if (closed) {
      // Walk the ring as an open line that comes back to its first point, so the closing side counts.
      ring = [...pts, pts[0]];
      ws = ws ? [...ws, ws[0]] : null;
    }

    let { runs, touched } = cutOpen(ring, ws, path, radius);
    if (!touched) continue;

    if (closed && runs.length > 1) {
      // The first and last runs meet at the ring's start point when it was kept: they are one stretch.
      const first = runs[0];
      const last = runs[runs.length - 1];
      const startKept = first.pts[0][0] === ring[0][0] && first.pts[0][1] === ring[0][1];
      const endKept = last.pts[last.pts.length - 1] === ring[ring.length - 1] || (last.pts[last.pts.length - 1][0] === ring[0][0] && last.pts[last.pts.length - 1][1] === ring[0][1]);
      if (startKept && endKept) {
        const joined: Run = { pts: [...last.pts, ...first.pts.slice(1)], ws: last.ws && first.ws ? [...last.ws, ...first.ws.slice(1)] : null };
        runs = [joined, ...runs.slice(1, -1)];
      }
    }

    const kept = runs.filter((r) => r.pts.length >= 2 && runLength(r) >= radius / 4);
    if (kept.length === 0) {
      out.remove.push(line.id);
      continue;
    }
    let best = 0;
    let bestLen = -1;
    kept.forEach((r, i) => {
      const l = runLength(r);
      if (l > bestLen) {
        bestLen = l;
        best = i;
      }
    });
    kept.forEach((r, i) => {
      if (i === best) out.edits.push({ id: line.id, points: r.pts, widths: r.ws });
      else out.adds.push({ from: line.id, points: r.pts, widths: r.ws });
    });
  }
  return out;
}
