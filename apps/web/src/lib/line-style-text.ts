// How one line is drawn when it differs from its type's look, as the text it is stored in: `linetype;scale;density`, and for a
// leader also `;arrow;shape;frame;size`. Plain TypeScript (no screen state) so it is checked on its own; the engine checks and
// writes the same form (`cave-draw::normalize_style`).

export interface LineStyle {
  /** '' is the type's own pattern; otherwise solid, dashed, longdash, sparse or dashdot. */
  lt: string;
  /** Multiplies the length of the dashes and gaps. */
  scale: number;
  /** Multiplies how many marks or hatch lines an area has. */
  density: number;
  /** A leader's arrowhead: '' (the usual arrow), arrow, filled, dot or none. */
  arrow: string;
  /** A leader's line: '' (elbow), elbow, straight or curved. */
  shape: string;
  /** A leader's note: '' (none), none, box or underline. */
  frame: string;
  /** A leader's text size as a multiple of the usual (3 mm on paper). */
  size: number;
}

export const PLAIN: LineStyle = { lt: '', scale: 1, density: 1, arrow: '', shape: '', frame: '', size: 1 };

export const ARROW_LABELS: [string, string][] = [['', 'Arrow'], ['filled', 'Filled arrow'], ['dot', 'Dot'], ['none', 'None']];
export const SHAPE_LABELS: [string, string][] = [['', 'Elbow'], ['straight', 'Straight'], ['curved', 'Curved']];
export const FRAME_LABELS: [string, string][] = [['', 'No frame'], ['box', 'Box'], ['underline', 'Underline']];

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
  const [lt = '', scale = '1', density = '1', arrow = '', shape = '', frame = '', size = '1'] = text.split(';');
  const num = (v: string) => {
    const n = Number(v);
    return Number.isFinite(n) && n > 0 ? n : 1;
  };
  return { lt, scale: num(scale), density: num(density), arrow, shape, frame: frame === 'none' ? '' : frame, size: num(size) };
}

/** Writes a style the way it is stored (empty when it is the type's look). */
export function styleText(s: LineStyle): string {
  const plainLine = !s.lt && Math.abs(s.scale - 1) < 0.005 && Math.abs(s.density - 1) < 0.005;
  const plainLeader = !s.arrow && !s.shape && !s.frame && Math.abs(s.size - 1) < 0.005;
  if (plainLine && plainLeader) return '';
  const line = `${s.lt};${s.scale.toFixed(2)};${s.density.toFixed(2)}`;
  return plainLeader ? line : `${line};${s.arrow};${s.shape};${s.frame};${s.size.toFixed(2)}`;
}
