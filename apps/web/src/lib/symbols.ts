// Point symbols (stalactites, pits, springs, ...). A placed symbol is a stroke of type `sym-<name>`
// with two points: where it sits, and a second point that says which way is up for the symbol
// (its direction) and how big it is (its distance from the first point, as the symbol's radius).
// Drawing is plain lines and circles in the symbol's own unit space, so every canvas draws them
// the same way. Nothing here touches the engine: the catalogue is installed once it has loaded.

import { ENGINE_HEADS, frameOutline, placeHead } from './shapes.ts';

export type Prim = { t: 'circle'; c: [number, number]; r: number; fill?: boolean } | { t: 'poly'; pts: [number, number][]; closed?: boolean; fill?: boolean };

export interface SymbolDef {
  key: string;
  block: string;
  set: string;
  name: string;
  uis: string;
  category: string;
  layer: string;
  prims: Prim[];
}

let catalogue = new Map<string, SymbolDef>();

/** Remembers the catalogue the engine sent (call once the engine has loaded). */
export function setSymbolCatalogue(list: SymbolDef[]) {
  catalogue = new Map(list.map((s) => [s.key, s]));
}

/** True for a type that is a symbol rather than a line. */
export function isSymbolKind(kind: string | undefined): boolean {
  return !!kind && kind.startsWith('sym-');
}

/** The definition of a symbol type, if the catalogue has it. */
export function symbolDef(kind: string): SymbolDef | undefined {
  return catalogue.get(kind);
}

/** How a symbol maps onto a canvas: its centre, "up" direction (unit) and radius, all in pixels. */
export function placement(p0: [number, number], p1: [number, number]): { cx: number; cy: number; ux: number; uy: number; r: number } {
  const dx = p1[0] - p0[0];
  const dy = p1[1] - p0[1];
  const r = Math.hypot(dx, dy);
  return r < 1e-9 ? { cx: p0[0], cy: p0[1], ux: 0, uy: -1, r: 0 } : { cx: p0[0], cy: p0[1], ux: dx / r, uy: dy / r, r };
}

/** Where a symbol's own point `(u, v)` (y up) lands on the canvas. */
export function symbolPoint(pl: ReturnType<typeof placement>, u: number, v: number): [number, number] {
  // "right" of the symbol on a y-down canvas is (-uy, ux)
  return [pl.cx + pl.r * (u * -pl.uy + v * pl.ux), pl.cy + pl.r * (u * pl.ux + v * pl.uy)];
}

/** Draws one symbol at `p0` pointing toward `p1` (canvas pixels), with the context's stroke style. */
export function drawSymbol(g: CanvasRenderingContext2D, kind: string, p0: [number, number], p1: [number, number]): void {
  const def = catalogue.get(kind);
  const pl = placement(p0, p1);
  if (!def || pl.r < 1) return;
  g.fillStyle = g.strokeStyle; // solid parts are filled in the ink colour
  for (const p of def.prims) {
    g.beginPath();
    if (p.t === 'circle') {
      // A circle stays a circle under rotation; its centre moves with the symbol.
      const [x, y] = symbolPoint(pl, p.c[0], p.c[1]);
      g.arc(x, y, p.r * pl.r, 0, Math.PI * 2);
      if (p.fill) g.fill();
    } else {
      p.pts.forEach((q, i) => {
        const [x, y] = symbolPoint(pl, q[0], q[1]);
        if (i === 0) g.moveTo(x, y);
        else g.lineTo(x, y);
      });
      if (p.closed) g.closePath();
      if (p.fill && p.closed) g.fill();
    }
    g.stroke();
  }
}

/** True when the screen point is on a placed symbol (inside its circle, or on its aim line). */
export function hitsSymbol(p0: [number, number], p1: [number, number], x: number, y: number, pad = 4): boolean {
  const pl = placement(p0, p1);
  return Math.hypot(x - pl.cx, y - pl.cy) <= Math.max(pl.r, 6) + pad;
}

/** Draws a survey station the way a cave map shows one: a triangle, with a dot in it when the station is
 *  fixed (an anchor of the survey). `r` is its size in pixels; the caller sets the ink and line width. */
export function drawStationMarker(g: CanvasRenderingContext2D, x: number, y: number, fixed: boolean, r = 6): void {
  g.setLineDash([]);
  drawSymbol(g, fixed ? 'sym-fixed-point' : 'sym-uis-survey-point', [x, y], [x, y - r]);
}

/** True for a type that is placed by a click and a drag rather than drawn: a symbol, text or a callout. */
export function isStampKind(kind: string | undefined): boolean {
  return isSymbolKind(kind) || kind === 'text' || kind === 'callout';
}

/** True for a type that is nothing without its words (text). A leader's note may be left empty. */
export function needsWords(kind: string | undefined): boolean {
  return kind === 'text';
}

/** A leader laid out: the note's box, and for each arrow the leader line and the arrow head's barbs (all from the engine). */
export interface CalloutLayout {
  height: number;
  side: 'left' | 'right';
  note: [number, number, number, number];
  branches: [number, number][][];
  heads: [number, number][][];
}
type CalloutSource = (tips: [number, number][], at: [number, number], chars: number, shape: string, height: number) => CalloutLayout;
let calloutSource: CalloutSource | null = null;
/** Installs the engine's callout layout (call once it has loaded). */
export function setCalloutSource(f: CalloutSource) {
  calloutSource = f;
}

/** A leader's layout on the canvas: the arrows point at `tips` (the first is the dragged-from point), the note goes at `at`. */
function calloutLayout(tips: [number, number][], at: [number, number], text: string, o: LeaderLook, height?: number): CalloutLayout | null {
  if (!calloutSource || !tips.length || Math.hypot(at[0] - tips[0][0], at[1] - tips[0][1]) < 3) return null;
  // the text is a size of its own (3 mm on paper times the leader's size) when the view knows how big a paper millimetre is
  const h = height ?? (o.paperPx && o.paperPx > 0 ? 3 * o.paperPx * o.size : 0);
  return calloutSource(tips, at, Math.max(1, text.length), o.shape || 'elbow', h);
}

/** How a leader is drawn besides its line: its arrowhead, line, note frame and text size, and how big a paper millimetre is here. */
export interface LeaderLook {
  arrow: string;
  shape: string;
  frame: string;
  size: number;
  /** The note's switches: f (fill behind it), h (outline round the letters), n (no outline on the frame). */
  flags: string;
  paperPx?: number;
}

/** Reads a leader's look from a stored style (the part after the line's own three). */
export function leaderLook(style: string | undefined, paperPx?: number): LeaderLook {
  const [, , , arrow = '', shape = '', frame = '', size = '1', , flags = ''] = (style ?? '').split(';');
  const n = Number(size);
  return { arrow, shape, frame, size: Number.isFinite(n) && n > 0 ? n : 1, flags, paperPx };
}

/** Where a stamp's words go and how big: for text, at the first point, turned to the aim; for a symbol's label, to its right. */
function wordsPlacement(kind: string, pl: ReturnType<typeof placement>): { x: number; y: number; size: number; angle: number } {
  if (kind === 'text') return { x: pl.cx, y: pl.cy, size: Math.max(6, pl.r), angle: Math.atan2(pl.ux, -pl.uy) };
  return { x: pl.cx + 1.3 * pl.r, y: pl.cy, size: Math.max(7, pl.r * 0.9), angle: 0 };
}

/** A colour that shows against the stroke's: pale behind dark ink, dark behind pale ink (for a note's fill and the outline of its letters). */
function backing(g: CanvasRenderingContext2D): string {
  g.save();
  g.fillStyle = typeof g.strokeStyle === 'string' ? g.strokeStyle : '#000';
  const c = String(g.fillStyle);
  g.restore();
  const m = /^#([0-9a-f]{6})$/i.exec(c) ?? null;
  const v = m ? parseInt(m[1], 16) : 0;
  const lum = m ? (0.299 * (v >> 16) + 0.587 * ((v >> 8) & 255) + 0.114 * (v & 255)) / 255 : 0;
  return lum > 0.55 ? '#14181d' : '#fffdf8';
}

/** Draws one arrowhead at `tip` for a leader whose line leaves it along `branch`. */
function drawHead(g: CanvasRenderingContext2D, name: string, tip: [number, number], branch: [number, number][], barbs: [number, number][], height: number) {
  if (name === 'none') return;
  if (name === 'dot') {
    g.save();
    g.fillStyle = g.strokeStyle;
    g.beginPath();
    g.arc(tip[0], tip[1], Math.max(2, height * 0.22), 0, Math.PI * 2);
    g.fill();
    g.restore();
    return;
  }
  if (ENGINE_HEADS.includes(name)) {
    g.beginPath();
    barbs.forEach((q, i) => (i === 0 ? g.moveTo(q[0], q[1]) : g.lineTo(q[0], q[1])));
    if (name === 'filled') {
      g.closePath();
      g.save();
      g.fillStyle = g.strokeStyle;
      g.fill();
      g.restore();
    }
    g.stroke();
    return;
  }
  // a library head points the way the leader arrives: from the next point of the line into the tip
  const from = branch[1] ?? branch[0];
  const d = Math.hypot(tip[0] - from[0], tip[1] - from[1]) || 1;
  const parts = placeHead(name, tip, [(tip[0] - from[0]) / d, (tip[1] - from[1]) / d], Math.max(6, height * 0.6));
  for (const part of parts ?? []) {
    g.beginPath();
    part.pts.forEach((q, i) => (i === 0 ? g.moveTo(q[0], q[1]) : g.lineTo(q[0], q[1])));
    if (part.closed) g.closePath();
    if (part.fill) {
      g.save();
      g.fillStyle = g.strokeStyle;
      g.fill();
      g.restore();
    }
    if (part.stroke) g.stroke();
  }
}


/** A symbol's label is a leader's note: a third point says where the note sits, and the leader to the symbol shows once the note is pulled clear of it. */
const LABEL_LOOK: LeaderLook = { ...leaderLook(''), arrow: 'none', frame: 'none' };

/** True when a symbol carries its label as a leader note (words and a note point). */
function labelLeader(kind: string, text: string, more: [number, number][]): boolean {
  return isSymbolKind(kind) && !!text && more.length > 0;
}

/** True when the note is clear enough of the symbol to need a leader line to it. */
function noteIsFar(pl: ReturnType<typeof placement>, note: [number, number]): boolean {
  return Math.hypot(note[0] - pl.cx, note[1] - pl.cy) > 2.6 * Math.max(pl.r, 8);
}

/** True when screen (`x`, `y`) is on the words of a symbol's label note (so a press there can pick the note up, selected or not). */
export function hitsLabelNote(kind: string | undefined, text: string | undefined, p0: [number, number], p1: [number, number], note: [number, number] | undefined, x: number, y: number, pad = 5): boolean {
  if (!kind || !isSymbolKind(kind) || !text || !note) return false;
  const size = wordsPlacement(kind, placement(p0, p1)).size;
  return x >= note[0] - pad && x <= note[0] + text.length * size * 0.6 + pad && Math.abs(y - note[1]) <= size * 0.6 + pad;
}

/** The points of a new symbol with its label's note spot added (to the right of it, where the label has always sat), so the label is a draggable leader from the start. */
export function withLabelSpot<T extends number[]>(kind: string, points: T[], text: string): T[] {
  if (!isSymbolKind(kind) || !text.trim() || points.length !== 2) return points;
  const r = Math.hypot(points[1][0] - points[0][0], points[1][1] - points[0][1]);
  return [...points, [points[0][0] + 1.3 * r, points[0][1], ...points[0].slice(2)] as T];
}

/** Draws a symbol (with its label) or a text stroke at `p0`, aimed toward `p1`. A leader's `more` are the points its further arrows touch. */
export function drawStamp(g: CanvasRenderingContext2D, kind: string, p0: [number, number], p1: [number, number], text = '', leader: LeaderLook = leaderLook(''), more: [number, number][] = []): void {
  let labelHeight: number | undefined;
  const labelled = labelLeader(kind, text, more);
  if (labelled) {
    // the label is a leader's note: just words beside the symbol until pulled away, then a leader (no arrowhead) back to it
    drawSymbol(g, kind, p0, p1);
    const pl = placement(p0, p1);
    const w = wordsPlacement(kind, pl);
    const note = more[0];
    if (!noteIsFar(pl, note)) {
      g.save();
      g.fillStyle = g.strokeStyle;
      g.font = `${w.size}px system-ui, sans-serif`;
      g.textBaseline = 'middle';
      g.fillText(text, note[0], note[1]);
      g.restore();
      return;
    }
    kind = 'callout';
    leader = LABEL_LOOK;
    more = [];
    p1 = note;
    labelHeight = w.size;
  }
  if (kind === 'callout') {
    const tips = [p0, ...more];
    const l = calloutLayout(tips, p1, text, leader, labelHeight);
    if (!l) return;
    const back = backing(g);
    // the note's frame, behind everything else of the note
    const pad = l.height * 0.2;
    const [x1, y1, x2, y2] = l.note;
    const box = frameOutline(leader.frame, [x1, Math.min(y1, y2), x2, Math.max(y1, y2)], pad);
    if (box) {
      g.beginPath();
      box.pts.forEach((q, i) => (i === 0 ? g.moveTo(q[0], q[1]) : g.lineTo(q[0], q[1])));
      if (box.closed) g.closePath();
      if (leader.flags.includes('f') && box.closed) {
        g.save();
        g.globalAlpha *= 0.92;
        g.fillStyle = back;
        g.fill();
        g.restore();
      }
      if (!leader.flags.includes('n')) g.stroke();
    }
    l.branches.forEach((branch, i) => {
      g.beginPath();
      branch.forEach((q, k) => (k === 0 ? g.moveTo(q[0], q[1]) : g.lineTo(q[0], q[1])));
      g.stroke();
      drawHead(g, leader.arrow, tips[i], branch, l.heads[i] ?? [], l.height);
    });
    if (text) {
      const y = (l.note[1] + l.note[3]) / 2;
      g.save();
      g.font = `${l.height}px system-ui, sans-serif`;
      g.textBaseline = 'middle';
      g.textAlign = 'left';
      const ink = g.strokeStyle;
      if (leader.flags.includes('h')) {
        // an outline round the letters keeps them readable over linework and fills
        g.strokeStyle = back;
        g.lineWidth = Math.max(2, l.height * 0.28);
        g.lineJoin = 'round';
        g.strokeText(text, l.note[0], y);
      }
      g.fillStyle = ink;
      g.fillText(text, l.note[0], y);
      g.restore();
    }
    return;
  }
  const pl = placement(p0, p1);
  if (kind !== 'text') drawSymbol(g, kind, p0, p1);
  if (!text || pl.r < 1) return;
  const w = wordsPlacement(kind, pl);
  g.save();
  g.fillStyle = g.strokeStyle;
  g.font = `${w.size}px system-ui, sans-serif`;
  g.textBaseline = 'middle';
  g.translate(w.x, w.y);
  g.rotate(w.angle);
  g.fillText(text, 0, 0);
  g.restore();
}

/** True when the screen point is on a placed stamp: a symbol's body or label, or the words of a text stroke. */
export function hitsStamp(kind: string, text: string | undefined, p0: [number, number], p1: [number, number], x: number, y: number, pad = 4, leader: LeaderLook = leaderLook(''), more: [number, number][] = []): boolean {
  if (kind === 'callout') {
    const tips = [p0, ...more];
    const l = calloutLayout(tips, p1, text ?? '', leader);
    if (!l) return Math.hypot(x - p0[0], y - p0[1]) <= pad + 6;
    const inNote = x >= l.note[0] - pad && x <= l.note[2] + pad && y >= Math.min(l.note[1], l.note[3]) - pad && y <= Math.max(l.note[1], l.note[3]) + pad;
    if (inNote) return true;
    for (const branch of l.branches) {
      for (let i = 1; i < branch.length; i++) if (distToSegment(x, y, branch[i - 1], branch[i]) <= pad + 2) return true;
    }
    return false;
  }
  const pl = placement(p0, p1);
  if (!text) return hitsSymbol(p0, p1, x, y, pad);
  const w = wordsPlacement(kind, pl);
  if (labelLeader(kind, text, more)) {
    const note = more[0];
    const inWords = x >= note[0] - pad && x <= note[0] + text.length * w.size * 0.6 + pad && Math.abs(y - note[1]) <= w.size * 0.6 + pad;
    if (inWords || hitsSymbol(p0, p1, x, y, pad)) return true;
    return noteIsFar(pl, note) && distToSegment(x, y, p0, note) <= pad + 2;
  }
  // the words' box in their own frame: along the baseline and across it
  const dx = x - w.x;
  const dy = y - w.y;
  const along = dx * Math.cos(w.angle) + dy * Math.sin(w.angle);
  const across = -dx * Math.sin(w.angle) + dy * Math.cos(w.angle);
  const inWords = along >= -pad && along <= text.length * w.size * 0.6 + pad && Math.abs(across) <= w.size * 0.6 + pad;
  return inWords || (kind !== 'text' && hitsSymbol(p0, p1, x, y, pad));
}

function distToSegment(x: number, y: number, a: [number, number], b: [number, number]): number {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const len2 = dx * dx + dy * dy;
  const t = len2 < 1e-12 ? 0 : Math.max(0, Math.min(1, ((x - a[0]) * dx + (y - a[1]) * dy) / len2));
  return Math.hypot(x - (a[0] + t * dx), y - (a[1] + t * dy));
}
