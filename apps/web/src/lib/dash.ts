// A line pattern on paper, turned into the dashes a screen draws. Patterns are in millimetres on paper (positive a dash, negative a
// gap, zero a dot), so a map printed at 1:500 or 1:2000 has dashes of the same size on paper: the drawing's scale only decides how
// much ground that is. Pure, so it is checked without a screen.

/** How long a dot is drawn, in millimetres on paper (the engine's `DOT_MM`). */
export const DOT_MM = 0.4;
/** Below this many pixels a whole dash-and-gap period is mush, and the line is drawn solid instead. */
const MIN_PERIOD_PX = 3;
/** The longest a single dash or gap may be, so a view magnified enormously still draws a pattern (pixels). */
const MAX_PART_PX = 4000;

/**
 * The dash array for a context: `pattern` in paper millimetres, `paperPx` pixels per paper millimetre, `scale` a multiplier for this
 * line. An empty pattern, or one too fine to see, gives `[]` (solid). Dashes and gaps alternate starting with a dash, as a canvas
 * wants (a pattern that begins with a gap is rotated so it does).
 */
export function dashPx(pattern: readonly number[], paperPx: number, scale = 1, dotMm = DOT_MM): number[] {
  if (!pattern.length || !(paperPx > 0) || !(scale > 0)) return [];
  const parts = pattern.map((v) => (v === 0 ? dotMm : Math.abs(v)) * paperPx * scale);
  const period = parts.reduce((a, b) => a + b, 0);
  if (period < MIN_PERIOD_PX) return [];
  const out = parts.map((p) => Math.min(MAX_PART_PX, Math.max(0.5, p)));
  // the canvas wants dash, gap, dash, gap...: an odd number of parts repeats itself so they still alternate
  return out.length % 2 ? [...out, ...out] : out;
}

/** The first dash and gap of a pattern in metres on the ground at a drawing scale (1:`n`), for a line material that only knows one of each. */
export function dashMetres(pattern: readonly number[], n: number, scale = 1, dotMm = DOT_MM): { dash: number; gap: number } | null {
  if (!pattern.length || !(n > 0) || !(scale > 0)) return null;
  const mm = (v: number) => (v === 0 ? dotMm : Math.abs(v));
  const k = (n / 1000) * scale;
  return { dash: mm(pattern[0]) * k, gap: mm(pattern[1] ?? pattern[0]) * k };
}
