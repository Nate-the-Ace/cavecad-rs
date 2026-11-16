// How a type of linework looks on a 2D canvas: the palette's colour role, weight and linetype,
// with the colour read from the theme now in force. One place, so the Profile, Cross Section and
// Map draw a wall, a ledge or a pit the same way.
import type { TypeStyle } from './engine';
import { isDark } from './theme';
import { parseStyle, patternName, drawing } from './line-style.svelte';
import { dashPx } from './dash';
import { inkCss } from './ink-colors';

let palette = new Map<string, TypeStyle>();

/** The theme's five colours on white paper (a printed sheet, whatever the screen's theme). */
const PAPER_THEME = { text: '#111111', muted: '#6b6b6b', accent: '#1565c0', warn: '#c26a00', danger: '#c62828' };

/** Remembers the palette the engine sent (call once the engine has loaded). */
export function setInkPalette(styles: TypeStyle[]) {
  palette = new Map(styles.map((s) => [s.key, s]));
}

/** How to stroke one kind of line on a canvas right now. */
export interface Look {
  color: string;
  /** Line width in pixels at pen weight 1. */
  weight: number;
  dash: number[];
  alpha: number;
  /** The name of the caver's own linetype this line is drawn with, if any: its marks replace the dashes. */
  custom?: string;
  /** Pixels in a paper millimetre where this line is (with the line's own scale), for laying a custom linetype out. */
  pxPerMm?: number;
}

/**
 * A function that gives the look of any kind, with the theme's colours read once. The pattern is on paper, so its pixels come from
 * `pxPerM` (how many pixels one metre of ground is where the line is) and the drawing's scale; `paper` says instead how many pixels a
 * paper millimetre is (a printed sheet), whatever the ground.
 */
export function looks(paper?: { pxPerMm: number }, onWhite = false): (kind: string, style?: string, pxPerM?: number) => Look {
  const css = getComputedStyle(document.documentElement);
  const v = (n: string) => css.getPropertyValue(n).trim();
  // a sheet is white paper whatever the theme: its colours are the light ones
  const theme = onWhite ? PAPER_THEME : { text: v('--text'), muted: v('--muted'), accent: v('--accent'), warn: v('--warn'), danger: v('--danger') };
  const dark = onWhite ? false : isDark();
  const ink = (role: TypeStyle['ink']) => inkCss(role, dark, theme);
  return (kind, style, pxPerM) => {
    const st = palette.get(kind);
    const own = style ? parseStyle(style) : null;
    // this line's own pattern, if it has one, else its type's; its scale lengthens or shortens both dash and gap
    const pattern = own?.lt ? (drawing.patterns[own.lt] ?? []) : (st?.pattern ?? []);
    const paperPx = paper ? paper.pxPerMm : pxPerM !== undefined ? (pxPerM * drawing.scale) / 1000 : 0;
    const dash = dashPx(pattern, paperPx, own?.scale ?? 1, drawing.dotMm);
    const custom = patternName(own?.lt);
    if (custom) return { color: ink(st ? st.ink : 'text'), weight: st?.weight ?? 1.8, dash: [], alpha: st?.opacity ?? 1, custom, pxPerMm: paperPx * (own?.scale ?? 1) };
    if (!st) return { color: ink('text'), weight: 1.8, dash, alpha: 1 };
    return { color: ink(st.ink), weight: st.weight, dash, alpha: st.opacity };
  };
}
