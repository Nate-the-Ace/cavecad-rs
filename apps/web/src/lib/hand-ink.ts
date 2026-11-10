// Ink style: every line drawn as if inked by hand. A drawn curve (already on screen, in pixels) gets
// a slight wobble across it, a slow swell and thinning of the pen, tapered ends and, on long lines, a
// rare short lift of the pen. Only the picture changes: the stored line, its snapping, picking and
// export are untouched.
//
// Everything is measured in paper millimetres along the line and seeded by the line's id, so the
// same line wobbles the same way on every redraw, at every zoom and on every device (a pan never
// makes it shimmer).

type Pt = [number, number];

/** How strong each effect is, in paper millimetres (or a fraction of the pen, for `swell`). */
export const INK = {
  /** Largest sideways wobble. */
  wobble: 0.22,
  /** Distance along the line of one slow wobble. */
  wobbleWave: 9,
  /** How far the pen's weight swells and thins (0.25 = 75% to 125%). */
  swell: 0.28,
  /** Length of the tapered start and end. */
  taper: 2.2,
  /** Pen at its thinnest, at the very tip of a taper. */
  tip: 0.3,
  /** A pen lift falls about this far apart on an open line. */
  liftEvery: 55,
  /** How long a pen lift is. */
  lift: 0.55,
};

/** A seed from a line's id: the same id always gives the same wobble. */
export function inkSeed(id: string): number {
  let h = 2166136261;
  for (let i = 0; i < id.length; i++) {
    h ^= id.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

/** Smooth 1D noise in about [-1, 1]: a sum of three sines whose phases come from the seed. */
function wave(seed: number, t: number): number {
  const ph = (k: number) => (((seed >>> (k * 8)) & 255) / 255) * Math.PI * 2;
  return 0.55 * Math.sin(t + ph(0)) + 0.3 * Math.sin(t * 2.31 + ph(1)) + 0.15 * Math.sin(t * 5.17 + ph(2));
}

/** A curve with a width factor per point; a width of 0 at both ends of a segment means the pen is lifted there. */
export interface InkCurve {
  pts: Pt[];
  widths: number[];
}

/**
 * The hand-inked version of a curve in screen pixels. `pxPerMm` is how many pixels one paper
 * millimetre is where the line is drawn; `widths` are the line's own (pen-pressure) widths, kept.
 * A closed line has no taper and no pen lifts (an outline should close).
 */
export function inkCurve(pts: Pt[], widths: number[] | null, id: string, pxPerMm: number, closed: boolean): InkCurve {
  const n = pts.length;
  const base = widths ?? pts.map(() => 1);
  if (n < 2 || !(pxPerMm > 0)) return { pts, widths: base };
  const seed = inkSeed(id || 'line');
  // arc length in paper millimetres at every point
  const s: number[] = [0];
  for (let i = 1; i < n; i++) s.push(s[i - 1] + Math.hypot(pts[i][0] - pts[i - 1][0], pts[i][1] - pts[i - 1][1]) / pxPerMm);
  const total = s[n - 1];
  if (total < 0.05) return { pts, widths: base };
  // the wobble fades out over the first and last millimetre so an end still meets what it was drawn to
  const pin = (d: number) => Math.min(1, Math.max(0, d / 1.0));
  const out: Pt[] = [];
  const w: number[] = [];
  const lifts = closed ? [] : liftsAlong(seed, total);
  for (let i = 0; i < n; i++) {
    const prev = pts[Math.max(0, i - 1)];
    const next = pts[Math.min(n - 1, i + 1)];
    let tx = next[0] - prev[0];
    let ty = next[1] - prev[1];
    const tl = Math.hypot(tx, ty) || 1;
    tx /= tl;
    ty /= tl;
    const t = (s[i] / INK.wobbleWave) * Math.PI * 2;
    const ends = closed ? 1 : Math.min(pin(s[i]), pin(total - s[i]));
    const off = INK.wobble * wave(seed, t) * ends * pxPerMm;
    out.push([pts[i][0] - ty * off, pts[i][1] + tx * off]);
    // the pen: a slow swell, and a taper at an open line's two ends
    let f = 1 + INK.swell * wave(seed ^ 0x9e3779b9, t * 0.6);
    if (!closed) {
      const taper = Math.min(1, s[i] / INK.taper, (total - s[i]) / INK.taper);
      f *= INK.tip + (1 - INK.tip) * Math.sqrt(Math.max(0, taper));
    }
    const lifted = lifts.some(([a, b]) => s[i] >= a && s[i] <= b);
    w.push(lifted ? 0 : base[i] * f);
  }
  return { pts: out, widths: w };
}

/** Where the pen lifts along an open line `total` mm long: none on a short line, never near an end. */
function liftsAlong(seed: number, total: number): [number, number][] {
  const out: [number, number][] = [];
  if (total < INK.liftEvery * 0.8) return out;
  let r = seed || 1;
  const rnd = () => {
    r = (Math.imul(r, 48271) >>> 0) % 2147483647;
    return r / 2147483647;
  };
  let at = INK.liftEvery * (0.5 + rnd());
  while (at < total - 6) {
    if (at > 6) out.push([at, at + INK.lift * (0.7 + 0.6 * rnd())]);
    at += INK.liftEvery * (0.6 + 0.8 * rnd());
  }
  return out;
}
