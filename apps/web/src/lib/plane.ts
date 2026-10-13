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
