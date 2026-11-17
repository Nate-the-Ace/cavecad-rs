// The colour of each colour role (cave-style's Ink), on light and on dark. One table, so the map, the profile, the
// cross section and the symbols agree: every kind of thing has its own colour, and a symbol takes the colour of its
// family of lines (a pit symbol the brown of pit lines). Text, Muted, Accent, Warn and Danger follow the theme's own
// colours; the rest are fixed here.

export type InkRole =
  | 'text'
  | 'muted'
  | 'accent'
  | 'warn'
  | 'danger'
  | 'station'
  | 'geology'
  | 'survey'
  | 'splay'
  | 'sweep'
  | 'passage'
  | 'relief'
  | 'water'
  | 'formation'
  | 'biology'
  | 'archaeology'
  | 'rigging';

/** [on light, on dark] for the fixed roles. */
export const FIXED_INKS: Partial<Record<InkRole, [string, string]>> = {
  geology: ['#b0138a', '#e57fd0'], // joints, faults, bedding: magenta
  survey: ['#d1352b', '#ff6b5e'], // survey legs: red
  splay: ['#e08a2e', '#f5a65b'], // single splays: orange
  sweep: ['#12968a', '#3fd2c0'], // sweep shots: teal
  passage: ['#5a6f8f', '#9fb3d1'], // walls built from the survey: grey-blue
  relief: ['#8a5a2b', '#d2a26b'], // ledges, pits, slopes, boulders, floor: brown
  water: ['#1f6fd1', '#5aa9ff'], // water: blue
  formation: ['#b08a00', '#e6c35c'], // flowstone, rimstone, formations: gold
  biology: ['#2e8b3a', '#6fd27c'], // bats, roots, guano: green
  archaeology: ['#6b7d1f', '#b5c860'], // bones, human traces: olive
  rigging: ['#8e44ad', '#c58cf0'], // ropes, bolts, ladders, camps: purple
};

/** The colour of a role now: `theme` gives the theme's own five (read from its CSS variables). */
export function inkCss(role: InkRole, dark: boolean, theme: { text: string; muted: string; accent: string; warn: string; danger: string }): string {
  const fixed = FIXED_INKS[role];
  if (fixed) return fixed[dark ? 1 : 0];
  if (role === 'station') return theme.text;
  return theme[role as 'text' | 'muted' | 'accent' | 'warn' | 'danger'] ?? theme.text;
}
