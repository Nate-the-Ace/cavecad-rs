// The ground under the cave as the engine hands it over (`engine.surfaceTerrain`): a w x h grid of
// points in the cave frame, row by row, row 0 the northern edge of the grid. A NaN z is a hole.
// The grid is regular in UTM and turned by the convergence in the cave frame, so it is read back
// through the affine its corners make.

export interface Terrain {
  w: number;
  h: number;
  xyz: Float32Array;
}

/**
 * The ground's height at cave (x, y): bilinear between the grid's points, clamped to the grid's edge.
 * Holes are left out of the blend; null where every neighbour is a hole or the grid is too small.
 */
export function terrainSampler(t: Terrain): (x: number, y: number) => number | null {
  const { w, h, xyz } = t;
  if (w < 2 || h < 2 || xyz.length < w * h * 3) return () => null;
  const ox = xyz[0];
  const oy = xyz[1];
  const c = (w - 1) * 3; // the last point of the first row
  const r = (h - 1) * w * 3; // the first point of the last row
  const ax = (xyz[c] - ox) / (w - 1);
  const ay = (xyz[c + 1] - oy) / (w - 1);
  const bx = (xyz[r] - ox) / (h - 1);
  const by = (xyz[r + 1] - oy) / (h - 1);
  const det = ax * by - ay * bx;
  if (!(Math.abs(det) > 1e-12)) return () => null;
  const z = (col: number, row: number) => xyz[(row * w + col) * 3 + 2];
  return (x, y) => {
    const dx = x - ox;
    const dy = y - oy;
    const u = Math.min(w - 1, Math.max(0, (dx * by - dy * bx) / det));
    const v = Math.min(h - 1, Math.max(0, (ax * dy - ay * dx) / det));
    const c0 = Math.min(w - 2, Math.floor(u));
    const r0 = Math.min(h - 2, Math.floor(v));
    const fu = u - c0;
    const fv = v - r0;
    let sum = 0;
    let weight = 0;
    for (const [cc, rr, k] of [
      [c0, r0, (1 - fu) * (1 - fv)],
      [c0 + 1, r0, fu * (1 - fv)],
      [c0, r0 + 1, (1 - fu) * fv],
      [c0 + 1, r0 + 1, fu * fv],
    ]) {
      const q = z(cc, rr);
      if (!Number.isFinite(q)) continue;
      sum += q * k;
      weight += k;
    }
    if (weight > 1e-9) return sum / weight;
    // On a grid point next to holes: the nearest finite neighbour, if any.
    for (const [cc, rr] of [[c0, r0], [c0 + 1, r0], [c0, r0 + 1], [c0 + 1, r0 + 1]]) if (Number.isFinite(z(cc, rr))) return z(cc, rr);
    return null;
  };
}

/** Every `step`th point of the grid (both ways, keeping the last row and column), for drawing a big grid lighter. */
export function thinTerrain(t: Terrain, step: number): Terrain {
  if (step <= 1) return t;
  const cols: number[] = [];
  for (let c = 0; c < t.w; c += step) cols.push(c);
  if (cols[cols.length - 1] !== t.w - 1) cols.push(t.w - 1);
  const rows: number[] = [];
  for (let r = 0; r < t.h; r += step) rows.push(r);
  if (rows[rows.length - 1] !== t.h - 1) rows.push(t.h - 1);
  const xyz = new Float32Array(cols.length * rows.length * 3);
  let i = 0;
  for (const r of rows)
    for (const c of cols) {
      const k = (r * t.w + c) * 3;
      xyz[i++] = t.xyz[k];
      xyz[i++] = t.xyz[k + 1];
      xyz[i++] = t.xyz[k + 2];
    }
  return { w: cols.length, h: rows.length, xyz };
}

/** True when two grids hold the same points (holes match holes), so a recompute that changed nothing redraws nothing. */
export function sameTerrain(a: Terrain | null, b: Terrain | null): boolean {
  if (!a || !b) return a === b;
  if (a.w !== b.w || a.h !== b.h || a.xyz.length !== b.xyz.length) return false;
  for (let i = 0; i < a.xyz.length; i++) {
    const p = a.xyz[i];
    const q = b.xyz[i];
    if (p !== q && !(Number.isNaN(p) && Number.isNaN(q))) return false;
  }
  return true;
}
