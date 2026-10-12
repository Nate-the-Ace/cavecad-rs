// How a type of linework looks on a 2D canvas: the palette's colour role, weight and linetype,
// with the colour read from the theme now in force. One place, so the Profile, Cross Section and
// Map draw a wall, a ledge or a pit the same way.
import type { TypeStyle } from './engine';

let palette = new Map<string, TypeStyle>();

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
}

/** A function that gives the look of any kind, with the theme's colours read once. */
export function looks(): (kind: string) => Look {
  const css = getComputedStyle(document.documentElement);
  const v = (n: string) => css.getPropertyValue(n).trim();
  const inks: Record<TypeStyle['ink'], string> = { text: v('--text'), muted: v('--muted'), accent: v('--accent'), warn: v('--warn'), danger: v('--danger'), station: v('--text') };
  return (kind) => {
    const st = palette.get(kind);
    if (!st) return { color: inks.text, weight: 1.8, dash: [], alpha: 1 };
    return { color: inks[st.ink], weight: st.weight, dash: st.linetype === 'dashed' ? [6, 4] : [], alpha: st.opacity };
  };
}
