// A rubber-sheet scan: the engine gives a thin-plate spline taking a scan pixel to the view's metres,
// exactly through every match. Here it is evaluated at the corners of a grid laid over the scan, and
// the picture is drawn grid cell by grid cell (two triangles each), so it bends with the spline.

/** The engine's spline (see `Tps` in cave-scans). */
export interface Warp {
  origin: [number, number];
  scale: number;
  centers: [number, number][];
  wx: number[];
  wy: number[];
  ax: [number, number, number];
  ay: [number, number, number];
}

/** Where a scan pixel lands in the view (metres). */
export function warpAt(w: Warp, px: number, py: number): [number, number] {
  const u = (px - w.origin[0]) / w.scale;
  const v = (py - w.origin[1]) / w.scale;
  let x = w.ax[0] + w.ax[1] * u + w.ax[2] * v;
  let y = w.ay[0] + w.ay[1] * u + w.ay[2] * v;
  for (let i = 0; i < w.centers.length; i++) {
    const r2 = (u - w.centers[i][0]) ** 2 + (v - w.centers[i][1]) ** 2;
    if (r2 < 1e-18) continue;
    const k = 0.5 * r2 * Math.log(r2);
    x += w.wx[i] * k;
    y += w.wy[i] * k;
  }
  return [x, y];
}

/** A grid over a box of the scan: `cols` x `rows` cells, nodes row by row. */
export interface Grid {
  cols: number;
  rows: number;
  /** Scan pixel of each node. */
  src: [number, number][];
  /** Where each node lands in the view (metres). */
  dst: [number, number][];
}

const CELLS = 24;

/** The grid over the box `[x0, y0, w, h]` of the scan, carried through the warp. */
export function warpGrid(w: Warp, box: { x0: number; y0: number; w: number; h: number }): Grid {
  const cols = CELLS;
  const rows = CELLS;
  const src: [number, number][] = [];
  const dst: [number, number][] = [];
  for (let j = 0; j <= rows; j++) {
    for (let i = 0; i <= cols; i++) {
      const p: [number, number] = [box.x0 + (box.w * i) / cols, box.y0 + (box.h * j) / rows];
      src.push(p);
      dst.push(warpAt(w, p[0], p[1]));
    }
  }
  return { cols, rows, src, dst };
}

/**
 * Draws the picture through the grid: each cell's two triangles are carried by their own affine map
 * from the scan to the screen. `to` maps the view's metres to screen pixels.
 */
export function drawWarped(g: CanvasRenderingContext2D, img: CanvasImageSource, grid: Grid, to: (x: number, y: number) => [number, number]) {
  const screen = grid.dst.map((d) => to(d[0], d[1]));
  const tri = (a: number, b: number, c: number) => {
    const [s0, s1, s2] = [grid.src[a], grid.src[b], grid.src[c]];
    const [d0, d1, d2] = [screen[a], screen[b], screen[c]];
    const det = (s1[0] - s0[0]) * (s2[1] - s0[1]) - (s2[0] - s0[0]) * (s1[1] - s0[1]);
    if (Math.abs(det) < 1e-9) return;
    // the affine map taking the scan triangle to the screen triangle
    const m11 = ((d1[0] - d0[0]) * (s2[1] - s0[1]) - (d2[0] - d0[0]) * (s1[1] - s0[1])) / det;
    const m21 = ((d2[0] - d0[0]) * (s1[0] - s0[0]) - (d1[0] - d0[0]) * (s2[0] - s0[0])) / det;
    const m12 = ((d1[1] - d0[1]) * (s2[1] - s0[1]) - (d2[1] - d0[1]) * (s1[1] - s0[1])) / det;
    const m22 = ((d2[1] - d0[1]) * (s1[0] - s0[0]) - (d1[1] - d0[1]) * (s2[0] - s0[0])) / det;
    g.save();
    // grow the triangle a hair from its centre so neighbouring cells overlap and leave no seam
    const cx = (d0[0] + d1[0] + d2[0]) / 3;
    const cy = (d0[1] + d1[1] + d2[1]) / 3;
    const grow = (d: [number, number]) => [d[0] + (d[0] - cx) * 0.02 + Math.sign(d[0] - cx) * 0.4, d[1] + (d[1] - cy) * 0.02 + Math.sign(d[1] - cy) * 0.4];
    const [e0, e1, e2] = [grow(d0), grow(d1), grow(d2)];
    g.beginPath();
    g.moveTo(e0[0], e0[1]);
    g.lineTo(e1[0], e1[1]);
    g.lineTo(e2[0], e2[1]);
    g.closePath();
    g.clip();
    g.transform(m11, m12, m21, m22, d0[0] - m11 * s0[0] - m21 * s0[1], d0[1] - m12 * s0[0] - m22 * s0[1]);
    g.drawImage(img, 0, 0);
    g.restore();
  };
  for (let j = 0; j < grid.rows; j++) {
    for (let i = 0; i < grid.cols; i++) {
      const a = j * (grid.cols + 1) + i;
      const b = a + 1;
      const c = a + grid.cols + 1;
      const d = c + 1;
      tri(a, b, c);
      tri(b, d, c);
    }
  }
}
