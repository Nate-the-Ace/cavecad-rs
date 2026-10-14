// Drawing scales and the grid that goes with them, as a draughtsman means them: "1 inch = 20 feet" is a
// statement about PAPER, so a scale is turned into pixels with the CSS inch (96 pixels), the same inch a
// printed sheet would use. Pure and tested (tests/sketchscale.check.ts).
//
// The grid is graph paper: a fine grid every tenth of a paper inch and a heavier line every inch, so at
// 1" = 20' the squares are 2 ft and the heavy lines 20 ft apart, and at 1" = 5' they are 6 in and 5 ft. It
// is fixed in the world (it moves with the drawing), not in the pane.

/** Pixels in a paper inch. The CSS inch: it is what a browser prints at, and it is the best a web page can know. */
export const PX_PER_INCH = 96;
const M_PER_FOOT = 0.3048;
const M_PER_INCH = 0.0254;

export interface ScalePreset {
  id: string;
  /** What the dropdown shows. */
  label: string;
  /** Pixels per metre of ground. */
  pxPerM: number;
  /** Fine grid spacing and heavy grid spacing, metres. */
  minorM: number;
  majorM: number;
}

const feet = (n: number): ScalePreset => ({
  id: `ft${n}`,
  label: `1" = ${n}'`,
  pxPerM: PX_PER_INCH / (n * M_PER_FOOT),
  minorM: (n / 10) * M_PER_FOOT,
  majorM: n * M_PER_FOOT,
});

const ratio = (n: number): ScalePreset => {
  // Metric: 1 : n. Fine grid 2 mm of paper, heavy line every 10 mm.
  const pxPerM = PX_PER_INCH / M_PER_INCH / n;
  return { id: `r${n}`, label: `1 : ${n}`, pxPerM, minorM: (n * 0.002), majorM: n * 0.01 };
};

/** The scales offered, 1" = 20' first: the default. */
export const SCALES: ScalePreset[] = [feet(20), feet(5), feet(10), feet(30), feet(40), feet(50), feet(100), ratio(100), ratio(200), ratio(500), ratio(1000)];
export const DEFAULT_SCALE = SCALES[0];

/** The preset a pixels-per-metre figure is (within half a percent), or `null` for a scale the person zoomed to. */
export function matchScale(pxPerM: number): ScalePreset | null {
  return SCALES.find((s) => Math.abs(s.pxPerM - pxPerM) / s.pxPerM < 0.005) ?? null;
}

/** "1\" = 17'" for any scale, in feet when it is an imperial-sized one, else as a ratio. */
export function describeScale(pxPerM: number): string {
  const preset = matchScale(pxPerM);
  if (preset) return preset.label;
  const feetPerInch = PX_PER_INCH / pxPerM / M_PER_FOOT;
  if (feetPerInch >= 1 && feetPerInch <= 400) return `1" = ${feetPerInch >= 10 ? Math.round(feetPerInch) : +feetPerInch.toFixed(1)}'`;
  return `1 : ${Math.round(PX_PER_INCH / M_PER_INCH / pxPerM)}`;
}

const NICE = [0.01, 0.02, 0.05, 0.1, 0.2, 0.5, 1, 2, 5, 10, 20, 50, 100, 200, 500, 1000];

/**
 * Fine and heavy grid spacing (metres) for any scale: the preset's own, or the smallest round figure that is
 * at least `minPx` pixels apart. Round in feet when the person is working in feet (`imperial`), else in metres,
 * so a zoomed view never shows a grid of 3.28 ft.
 */
export function gridFor(pxPerM: number, minPx = 7.5, imperial = false): { minorM: number; majorM: number } {
  const preset = matchScale(pxPerM);
  if (preset) return { minorM: preset.minorM, majorM: preset.majorM };
  const unit = imperial ? M_PER_FOOT : 1;
  const minor = (NICE.find((m) => m * unit * pxPerM >= minPx) ?? 1000) * unit;
  // A heavy line every fifth fine line when the fine spacing starts with 2, else every tenth.
  const n = minor / unit;
  const lead = n / 10 ** Math.floor(Math.log10(n));
  return { minorM: minor, majorM: minor * (Math.abs(lead - 2) < 1e-9 ? 5 : 10) };
}

/** Grid line positions (metres) covering [from, to] at `step`, each tagged heavy when it falls on a multiple of `major`. */
export function gridLines(from: number, to: number, step: number, major: number, maxLines = 400): { at: number; heavy: boolean }[] {
  if (!(step > 0) || !(to > from)) return [];
  const first = Math.ceil(from / step - 1e-9);
  const last = Math.floor(to / step + 1e-9);
  if (last - first > maxLines) return [];
  const out: { at: number; heavy: boolean }[] = [];
  for (let i = first; i <= last; i++) {
    const at = i * step;
    const k = at / major;
    out.push({ at, heavy: Math.abs(k - Math.round(k)) < 1e-6 });
  }
  return out;
}

/** Length of a grid step in the unit people read it in: "2 ft", "6 in", "0.5 m". */
export function stepText(m: number, imperial: boolean): string {
  if (imperial) {
    const ft = m / M_PER_FOOT;
    return ft >= 0.99 ? `${+ft.toFixed(2)} ft` : `${Math.round(ft * 12)} in`;
  }
  return m >= 1 ? `${+m.toFixed(2)} m` : `${Math.round(m * 100)} cm`;
}
