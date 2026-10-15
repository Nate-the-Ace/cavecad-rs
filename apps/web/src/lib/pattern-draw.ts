// Drawing the patterns a caver authored on a canvas: a custom linetype along a line, a custom fill inside an area. The engine lays
// the marks out; this file asks for them (keeping the answers until the line, the scale or the pattern changes) and strokes them.
import type { PatternMarks } from './patterns';

/** Where the marks come from: the engine, which installs itself once it has loaded (so this file does not import it, and can be loaded without it). */
export interface PatternSource {
  revision(): number;
  linePatternMarksNamed(name: string, pts: [number, number][], scale: number): PatternMarks;
  fillPatternMarksNamed(name: string, outline: [number, number][], id: string, amount: number, scale: number): PatternMarks;
}
// `var`, with no initial value: see section-glyph.ts (the modules import each other in a ring while loading).
var engine: PatternSource | undefined;
/** Installs the source of pattern marks. */
export function setPatternSource(s: PatternSource) {
  engine = s;
}

const lines = new Map<string, PatternMarks>();
const EMPTY: PatternMarks = {};

function keyOf(kind: string, name: string, scale: number, pts: [number, number][], extra = ''): string {
  const a = pts[0];
  const b = pts[pts.length >> 1];
  const c = pts[pts.length - 1];
  // the cave's revision goes up on every edit, undo and merge, so a changed pattern is never drawn from an old layout
  return `${engine?.revision() ?? 0}|${kind}|${name}|${scale.toFixed(3)}|${extra}|${pts.length}|${a}|${b}|${c}`;
}

/** The marks of a custom linetype along `pts` (pixels). `pxPerMm` is how many pixels a paper millimetre is here. */
export function lineMarksPx(name: string, pts: [number, number][], pxPerMm: number, scale = 1): PatternMarks {
  if (!engine || pts.length < 2 || !(pxPerMm > 0)) return EMPTY;
  // the engine's layout is in whatever units the path is in: giving it pixels and a "scale" that makes a paper millimetre
  // `pxPerMm` of them lays the pattern out in pixels directly
  const s = pxPerMm * 1000 * scale;
  const k = keyOf('line', name, s, pts);
  let m = lines.get(k);
  if (!m) {
    m = engine.linePatternMarksNamed(name, pts, s);
    if (lines.size > 600) lines.clear();
    lines.set(k, m);
  }
  return m;
}

/** The marks of a custom fill inside `outline` (metres) at the drawing's scale. */
export function fillMarksM(name: string, id: string, outline: [number, number][], amount: number, scale: number): PatternMarks {
  if (!engine || outline.length < 3) return EMPTY;
  const k = keyOf('fill', name, scale, outline, `${id}|${amount}`);
  let m = lines.get(k);
  if (!m) {
    m = engine.fillPatternMarksNamed(name, outline, id, amount, scale);
    if (lines.size > 600) lines.clear();
    lines.set(k, m);
  }
  return m;
}

/** Strokes marks that are already in pixels (a custom linetype). Circles are `[x, y, r, filled]`. */
export function strokeMarksPx(g: CanvasRenderingContext2D, marks: PatternMarks) {
  for (const line of marks.lines ?? []) {
    if (line.length < 2) continue;
    g.beginPath();
    g.moveTo(line[0][0], line[0][1]);
    for (let i = 1; i < line.length; i++) g.lineTo(line[i][0], line[i][1]);
    g.stroke();
  }
  for (const [x, y, r, filled] of marks.circles ?? []) {
    g.beginPath();
    g.arc(x, y, Math.max(r, 0.5), 0, Math.PI * 2);
    if (filled) g.fill();
    else g.stroke();
  }
}
