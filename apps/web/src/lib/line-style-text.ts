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
  /** Turns an area's pattern inside its boundary, in degrees (-90 to 90); the boundary does not move. */
  angle: number;
  /** A leader's note switches, as letters: f (a paper fill behind the note), h (an outline round the letters), n (no outline on the frame). */
  flags: string;
  /** Where a leader's arrows are fastened to other drawn things: `index=id#vertex` pairs, comma-separated (see `leader-edit.ts`). */
  attach: string;
  /** A note's type face: '' (sans), serif or mono. */
  font: string;
  /** A note's colour as `#rrggbb`, or '' for the line's own ink. */
  color: string;
  /** Where a note's words sit against the point they are placed at: '' (left), center or right. */
  align: string;
}

/** The switches a note may have, as letters in the order they are stored: b bold, f fill, h letter outline, i italic, n no frame outline, u underline. */
export const NOTE_FLAGS = 'bfhinu';

export const PLAIN: LineStyle = { lt: '', scale: 1, density: 1, arrow: '', shape: '', frame: '', size: 1, angle: 0, flags: '', attach: '', font: '', color: '', align: '' };

export const FONT_LABELS: [string, string][] = [['', 'Sans'], ['serif', 'Serif'], ['mono', 'Mono']];
export const ALIGN_LABELS: [string, string][] = [['', 'Left'], ['center', 'Centre'], ['right', 'Right']];

/** The flags with `letter` switched on or off. */
export function withFlag(flags: string, letter: string, on: boolean): string {
  return [...NOTE_FLAGS].filter((c) => (c === letter ? on : flags.includes(c))).join('');
}

export const ARROW_LABELS: [string, string][] = [
  ['', 'Open arrow'],
  ['filled', 'Filled arrow'],
  ['closed', 'Closed arrow'],
  ['stealth', 'Stealth'],
  ['diamond', 'Diamond'],
  ['open-diamond', 'Open diamond'],
  ['square', 'Square'],
  ['dot', 'Dot'],
  ['tick', 'Tick'],
  ['bar', 'Bar'],
  ['double', 'Double arrow'],
  ['none', 'No head'],
];
export const SHAPE_LABELS: [string, string][] = [['', 'Elbow'], ['straight', 'Straight'], ['curved', 'Curved']];
export const FRAME_LABELS: [string, string][] = [
  ['', 'No frame'],
  ['box', 'Box'],
  ['rounded', 'Rounded box'],
  ['hexagon', 'Hexagon'],
  ['octagon', 'Octagon'],
  ['diamond', 'Diamond'],
  ['ellipse', 'Ellipse'],
  ['banner', 'Banner'],
  ['underline', 'Underline'],
];

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
  const [lt = '', scale = '1', density = '1', arrow = '', shape = '', frame = '', size = '1', angle = '0', flags = '', attach = '', font = '', color = '', align = ''] = text.split(';');
  const num = (v: string) => {
    const n = Number(v);
    return Number.isFinite(n) && n > 0 ? n : 1;
  };
  return { lt, scale: num(scale), density: num(density), arrow, shape, frame: frame === 'none' ? '' : frame, size: num(size), angle: Number.isFinite(Number(angle)) ? Number(angle) : 0, flags, attach, font, color, align: align === 'left' ? '' : align };
}

/** Writes a style the way it is stored (empty when it is the type's look). */
export function styleText(s: LineStyle): string {
  const plainLine = !s.lt && Math.abs(s.scale - 1) < 0.005 && Math.abs(s.density - 1) < 0.005;
  const plainLeader = !s.arrow && !s.shape && !s.frame && Math.abs(s.size - 1) < 0.005;
  const turned = Math.abs(s.angle) >= 0.05;
  const flags = [...NOTE_FLAGS].filter((c) => s.flags.includes(c)).join('');
  const align = s.align === 'left' ? '' : s.align;
  if (s.font || s.color || align) return `${s.lt};${s.scale.toFixed(2)};${s.density.toFixed(2)};${s.arrow};${s.shape};${s.frame};${s.size.toFixed(2)};${s.angle.toFixed(1)};${flags};${s.attach};${s.font};${s.color};${align}`;
  if (s.attach) return `${s.lt};${s.scale.toFixed(2)};${s.density.toFixed(2)};${s.arrow};${s.shape};${s.frame};${s.size.toFixed(2)};${s.angle.toFixed(1)};${flags};${s.attach}`;
  if (flags) return `${s.lt};${s.scale.toFixed(2)};${s.density.toFixed(2)};${s.arrow};${s.shape};${s.frame};${s.size.toFixed(2)};${s.angle.toFixed(1)};${flags}`;
  if (plainLine && plainLeader && !turned) return '';
  const line = `${s.lt};${s.scale.toFixed(2)};${s.density.toFixed(2)}`;
  if (plainLeader && !turned) return line;
  const full = `${line};${s.arrow};${s.shape};${s.frame};${s.size.toFixed(2)}`;
  return turned ? `${full};${s.angle.toFixed(1)}` : full;
}

/** A style with a leader's fastenings replaced (nothing else changes). */
export function withAttach(style: string | undefined, attach: string): string {
  return styleText({ ...parseStyle(style), attach });
}

/** The style text of a pattern the caver authored: `pat:Name` in the linetype part (see `cave-draw::normalize_style`). */
export const PATTERN_PREFIX = 'pat:';

/** The pattern's name when `lt` (the linetype part of a style) names one of the caver's, else ''. */
export function patternName(lt: string | undefined): string {
  return lt && lt.startsWith(PATTERN_PREFIX) ? lt.slice(PATTERN_PREFIX.length) : '';
}
