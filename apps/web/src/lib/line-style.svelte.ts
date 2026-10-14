// How one line is drawn when it differs from its type's look: its own linetype, a scale for the dashes and a density for an
// area's hatch. Stored on the line as `linetype;scale;density` (empty for the type's look). While a slider is being dragged the
// new value is held here and drawn at once; it is saved as one undoable edit when the slider is let go.

export interface LineStyle {
  /** '' is the type's own pattern; otherwise solid, dashed, longdash, sparse or dashdot. */
  lt: string;
  /** Multiplies the length of the dashes and gaps. */
  scale: number;
  /** Multiplies how many marks or hatch lines an area has. */
  density: number;
}

export const PLAIN: LineStyle = { lt: '', scale: 1, density: 1 };

export const LINETYPE_LABELS: [string, string][] = [
  ['', 'The type’s own'],
  ['solid', 'Solid'],
  ['dashed', 'Dashed'],
  ['longdash', 'Long dash'],
  ['sparse', 'Sparse'],
  ['dashdot', 'Dash dot'],
];

/** Reads a stored style (anything unreadable is the type's look). */
export function parseStyle(text: string | undefined): LineStyle {
  if (!text) return PLAIN;
  const [lt = '', scale = '1', density = '1'] = text.split(';');
  const s = Number(scale);
  const d = Number(density);
  return { lt, scale: Number.isFinite(s) && s > 0 ? s : 1, density: Number.isFinite(d) && d > 0 ? d : 1 };
}

/** Writes a style the way it is stored (empty when it is the type's look). */
export function styleText(s: LineStyle): string {
  if (!s.lt && Math.abs(s.scale - 1) < 0.005 && Math.abs(s.density - 1) < 0.005) return '';
  return `${s.lt};${s.scale.toFixed(2)};${s.density.toFixed(2)}`;
}

/** A style being dragged: drawn for this line instead of its stored one, until it is saved. */
export const liveStyle = $state({ id: '', style: '' });

/** The style to draw a line with: the one being dragged for it, or what is stored. */
export function styleFor(id: string | undefined, stored: string | undefined): string {
  return id && liveStyle.id === id ? liveStyle.style : (stored ?? '');
}
