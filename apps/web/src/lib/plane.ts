// The plane a scan lies in on the Map: z = a + b*x + c*y, fitted through the stations it is matched
// to so the scan touches them (as well as one flat sheet can). With no stations it is level at
// `fallbackZ`; with one, level through it; with stations in a line it follows the line and is level
// across it. A very steep fit (stations down a shaft) is held to a 45 degree slope so the scan never
// stands on its edge.

const MAX_SLOPE = 1;

export function fitPlane(pts: [number, number, number][], fallbackZ: number): [number, number, number] {
  const n = pts.length;
  if (n === 0) return [fallbackZ, 0, 0];
  const mx = pts.reduce((t, p) => t + p[0], 0) / n;
  const my = pts.reduce((t, p) => t + p[1], 0) / n;
  const mz = pts.reduce((t, p) => t + p[2], 0) / n;
  let sxx = 0, sxy = 0, syy = 0, sxz = 0, syz = 0;
  for (const p of pts) {
    const dx = p[0] - mx, dy = p[1] - my, dz = p[2] - mz;
    sxx += dx * dx; sxy += dx * dy; syy += dy * dy; sxz += dx * dz; syz += dy * dz;
  }
  let b = 0;
  let c = 0;
  const det = sxx * syy - sxy * sxy;
  if (n >= 3 && det > 1e-9 * Math.max(1, sxx * syy)) {
    b = (sxz * syy - syz * sxy) / det;
    c = (syz * sxx - sxz * sxy) / det;
  } else if (n >= 2 && sxx + syy > 1e-9) {
    // in a line: slope along it, level across
    // the direction of the spread: the leading eigenvector of [[sxx, sxy], [sxy, syy]]
    const tr = sxx + syy;
    const l1 = tr / 2 + Math.sqrt(Math.max(0, (tr / 2) ** 2 - det));
    let dx = sxy, dy = l1 - sxx;
    if (Math.hypot(dx, dy) < 1e-12) [dx, dy] = sxx >= syy ? [1, 0] : [0, 1];
    const d = Math.hypot(dx, dy);
    dx /= d; dy /= d;
    let t = 0, tt = 0;
    for (const p of pts) {
      const u = (p[0] - mx) * dx + (p[1] - my) * dy;
      t += u * (p[2] - mz);
      tt += u * u;
    }
    const slope = tt > 1e-12 ? t / tt : 0;
    b = slope * dx;
    c = slope * dy;
  }
  const g = Math.hypot(b, c);
  if (g > MAX_SLOPE) {
    b *= MAX_SLOPE / g;
    c *= MAX_SLOPE / g;
  }
  return [mz - b * mx - c * my, b, c];
}

/**
 * The height of the ground a scan is laid on, as a function of where on the map: a thin-plate surface
 * through the stations the scan is matched to (exact at each station, smooth between), held within
 * a little more than their range so it can never fly off beyond them. Stations on top of each other
 * are averaged; with fewer than three spread-out stations (or a fit that cannot be solved) it is
 * the flat or sloping plane of `fitPlane`.
 */
export function heightSurface(pts: [number, number, number][], fallbackZ: number): (x: number, y: number) => number {
  const plane = fitPlane(pts, fallbackZ);
  const flat = (x: number, y: number) => plane[0] + plane[1] * x + plane[2] * y;
  if (pts.length < 3) return flat;
  // average stations that share a spot
  const key = (p: [number, number, number]) => `${p[0].toFixed(3)},${p[1].toFixed(3)}`;
  const byXY = new Map<string, { x: number; y: number; zs: number[] }>();
  for (const p of pts) {
    const k = key(p);
    const e = byXY.get(k) ?? { x: p[0], y: p[1], zs: [] };
    e.zs.push(p[2]);
    byXY.set(k, e);
  }
  const c = [...byXY.values()].map((e) => ({ x: e.x, y: e.y, z: e.zs.reduce((a, b) => a + b, 0) / e.zs.length }));
  const n = c.length;
  if (n < 3) return flat;
  const mx = c.reduce((t, p) => t + p.x, 0) / n;
  const my = c.reduce((t, p) => t + p.y, 0) / n;
  const scale = Math.max(1e-6, ...c.map((p) => Math.hypot(p.x - mx, p.y - my)));
  const u = c.map((p) => [(p.x - mx) / scale, (p.y - my) / scale]);
  const k = (r2: number) => (r2 < 1e-18 ? 0 : 0.5 * r2 * Math.log(r2));
  const m = n + 3;
  const a: number[][] = Array.from({ length: m }, () => new Array(m + 1).fill(0));
  for (let i = 0; i < n; i++) {
    for (let j = 0; j < n; j++) a[i][j] = k((u[i][0] - u[j][0]) ** 2 + (u[i][1] - u[j][1]) ** 2);
    a[i][i] += 1e-9; // a touch of stiffness: no wild swings between close stations
    a[i][n] = 1;
    a[i][n + 1] = u[i][0];
    a[i][n + 2] = u[i][1];
    a[n][i] = 1;
    a[n + 1][i] = u[i][0];
    a[n + 2][i] = u[i][1];
    a[i][m] = c[i].z;
  }
  for (let col = 0; col < m; col++) {
    let piv = col;
    for (let r = col + 1; r < m; r++) if (Math.abs(a[r][col]) > Math.abs(a[piv][col])) piv = r;
    if (Math.abs(a[piv][col]) < 1e-12) return flat;
    [a[col], a[piv]] = [a[piv], a[col]];
    for (let r = 0; r < m; r++) {
      if (r === col) continue;
      const f = a[r][col] / a[col][col];
      if (f !== 0) for (let cc = col; cc <= m; cc++) a[r][cc] -= f * a[col][cc];
    }
  }
  const sol = a.map((row, i) => row[m] / row[i]);
  if (sol.some((v) => !Number.isFinite(v))) return flat;
  const zs = c.map((p) => p.z);
  const lo = Math.min(...zs);
  const hi = Math.max(...zs);
  const margin = Math.max(0.5, (hi - lo) * 0.2);
  return (x, y) => {
    const px = (x - mx) / scale;
    const py = (y - my) / scale;
    let z = sol[n] + sol[n + 1] * px + sol[n + 2] * py;
    for (let i = 0; i < n; i++) z += sol[i] * k((px - u[i][0]) ** 2 + (py - u[i][1]) ** 2);
    return Math.min(hi + margin, Math.max(lo - margin, z));
  };
}
