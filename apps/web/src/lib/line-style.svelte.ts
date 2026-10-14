// How one line is drawn when it differs from its type's look: its own linetype, a scale for the dashes and a density for an
// area's hatch. Stored on the line as `linetype;scale;density` (empty for the type's look). While a slider is being dragged the
// new value is held here and drawn at once; it is saved as one undoable edit when the slider is let go.

export { PATTERN_PREFIX, patternName, ALIGN_LABELS, FONT_LABELS, NOTE_FLAGS, withFlag, ARROW_LABELS, FRAME_LABELS, LINETYPE_LABELS, PLAIN, SHAPE_LABELS, parseStyle, styleText, type LineStyle } from './line-style-text';

/** A style being dragged: drawn for this line instead of its stored one, until it is saved. */
export const liveStyle = $state({ id: '', style: '' });

/** The style to draw a line with: the one being dragged for it, or what is stored. */
export function styleFor(id: string | undefined, stored: string | undefined): string {
  return id && liveStyle.id === id ? liveStyle.style : (stored ?? '');
}

/** The drawing's scale (the N of 1:N) and the line patterns the engine knows (on paper, in millimetres). Patterns and fills are sized on paper at this scale. */
export const drawing = $state({ scale: 500, patterns: {} as Record<string, number[]>, dotMm: 0.4 });

/** What the fills' authored sizes belong to: at 1:500 an area fills exactly as it was authored. */
export const BASE_SCALE = 500;

/** The factor for the fill marks: the drawing's scale over the base one. */
export const fillLength = (): number => drawing.scale / BASE_SCALE;

/** Takes in the drawing's scale and the linetypes from the engine (call when a cave opens or changes). */
export function readDrawing(read: { scale: number; types: { key: string; pattern: number[] }[]; dotMm: number }) {
  if (drawing.scale !== read.scale) drawing.scale = read.scale;
  if (!Object.keys(drawing.patterns).length) {
    drawing.patterns = Object.fromEntries(read.types.map((t) => [t.key, t.pattern]));
    drawing.dotMm = read.dotMm;
  }
}
