// What is new for a drawing at a paper scale: the grid, and turning "inches of paper to feet of ground" into
// screen pixels. Everything else comes from what the app already has: the list of scales is the Sheets
// tab's (`engine.sheetOptions().scales`), lengths and feet are `units.ts`. Pure and tested
// (tests/sketchgrid.check.ts).
//
// The grid is the same at every scale and every zoom: 2 ft squares with a heavy line every 20 ft, fixed in the
// world (it moves with the drawing). It is never re-spaced to suit the zoom; when it would be too fine to see,
// the fine lines are left out and the grid is still 2 ft.

import { FOOT, formatLength } from './units.ts';

/** Pixels in a paper inch. The CSS inch: it is what a browser prints at, and the best a web page can know of a screen. */
export const PX_PER_INCH = 96;

/** The scale a drawing opens at: 1 inch of paper to 20 feet of ground. */
export const DEFAULT_FEET_PER_INCH = 20;

/** Pixels per metre of ground at `feetPerInch` feet to the paper inch. */
export const pxPerMetre = (feetPerInch: number): number => PX_PER_INCH / (feetPerInch * FOOT);

/** The feet to the paper inch a pixels-per-metre figure is. */
export const feetPerInch = (pxPerM: number): number => PX_PER_INCH / (pxPerM * FOOT);

/** The grid: 2 ft squares, a heavy line every 20 ft (10 squares), in metres. */
export const GRID = { minorM: 2 * FOOT, majorM: 20 * FOOT } as const;

/** Fewest pixels between lines for them to be worth drawing: closer than this they only make a haze. */
export const MIN_GRID_PX = 4;

/** Which of the grid's lines are far enough apart to draw at this zoom (the spacing itself never changes). */
export function visibleGrid(pxPerM: number): { minor: boolean; major: boolean } {
  return { minor: GRID.minorM * pxPerM >= MIN_GRID_PX, major: GRID.majorM * pxPerM >= MIN_GRID_PX };
}

/** The scale (one of `scales`, the Sheets tab's list) a pixels-per-metre figure is, within half a percent; `null` for one zoomed to. */
export function matchScale<S extends { feet_per_inch: number }>(pxPerM: number, scales: readonly S[]): S | null {
  return scales.find((s) => Math.abs(pxPerMetre(s.feet_per_inch) - pxPerM) / pxPerM < 0.005) ?? null;
}

/** What to call a scale: the list's own label, or `1" = 17 ft` for one zoomed to. */
export function describeScale(pxPerM: number, scales: readonly { label: string; feet_per_inch: number }[]): string {
  const known = matchScale(pxPerM, scales);
  if (known) return known.label;
  const f = feetPerInch(pxPerM);
  return `1" = ${f >= 10 ? Math.round(f) : +f.toFixed(1)} ft`;
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

/** The grid's size as it is captioned: "2 ft / 20 ft". */
export const gridCaption = (): string => `${formatLength(GRID.minorM, 'imperial')} / ${formatLength(GRID.majorM, 'imperial')}`;

/**
 * Draws the grid over a pane. `toWorld` and the pane size give the visible area; `tx` and `ty` place the
 * origin. Fine lines are left out when closer than `MIN_GRID_PX`; the spacing never changes.
 */
export function drawGrid(g: CanvasRenderingContext2D, t: { scale: number; tx: number; ty: number }, w: number, h: number, ink: string): void {
  const shown = visibleGrid(t.scale);
  if (!shown.major) return;
  const x0 = (0 - t.tx) / t.scale;
  const x1 = (w - t.tx) / t.scale;
  const y1 = t.ty / t.scale;
  const y0 = (t.ty - h) / t.scale;
  const step = shown.minor ? GRID.minorM : GRID.majorM;
  g.save();
  g.lineWidth = 1;
  g.strokeStyle = ink;
  for (const heavy of [false, true]) {
    g.globalAlpha = heavy ? 0.2 : 0.075;
    g.beginPath();
    for (const l of gridLines(x0, x1, step, GRID.majorM)) {
      if (l.heavy !== heavy) continue;
      const x = Math.round(t.tx + l.at * t.scale) + 0.5;
      g.moveTo(x, 0);
      g.lineTo(x, h);
    }
    for (const l of gridLines(y0, y1, step, GRID.majorM)) {
      if (l.heavy !== heavy) continue;
      const y = Math.round(t.ty - l.at * t.scale) + 0.5;
      g.moveTo(0, y);
      g.lineTo(w, y);
    }
    g.stroke();
  }
  g.restore();
}
