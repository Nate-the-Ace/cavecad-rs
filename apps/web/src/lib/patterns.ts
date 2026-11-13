// Patterns the caver authors: motifs (small drawings), linetypes (dashes, dots and motifs repeated along a line) and fills
// (hatches, dots and tiled motifs repeated inside an outline). The engine owns the definitions and lays them out; this file has
// their shapes, the starting values for new ones, and the drawing of laid-out marks.

export type PatternKind = 'motif' | 'line' | 'fill';

/** One drawn piece of a motif or symbol, in unit coordinates (x along the line, y across it, y up). */
export type Prim =
  | { t: 'circle'; c: [number, number]; r: number; fill?: boolean }
  | { t: 'poly'; pts: [number, number][]; closed?: boolean; fill?: boolean };

export interface Motif {
  name: string;
  prims: Prim[];
  /** True for the cave's own (can be changed and removed); the built-in ones are copied to change. */
  own?: boolean;
}

export type Element =
  | { t: 'dash'; mm: number }
  | { t: 'gap'; mm: number }
  | { t: 'dot' }
  | { t: 'motif'; motif: string; size_mm: number; turn_deg: number; along_mm: number; across_mm: number }
  | { t: 'text'; text: string; size_mm: number; turn_deg: number; along_mm: number; across_mm: number };

export interface LineDef {
  name: string;
  elements: Element[];
  own?: boolean;
}

export type Layer =
  | { t: 'hatch'; angle_deg: number; spacing_mm: number }
  | { t: 'dots'; spacing_mm: number; dot_mm: number }
  | {
      t: 'tile';
      motif: string;
      size_mm: number;
      across_mm: number;
      down_mm: number;
      stagger: boolean;
      turn_deg: number;
      random_turn: boolean;
      size_var: number;
      jitter: number;
      skip: number;
    }
  | { t: 'patlines'; angle_deg: number; x_mm: number; y_mm: number; shift_mm: number; spacing_mm: number; dashes_mm: number[] };

export interface FillDef {
  name: string;
  layers: Layer[];
  tint: boolean;
  own?: boolean;
}

export interface PatternLibrary {
  motifs: Motif[];
  lines: LineDef[];
  fills: FillDef[];
}

/** The marks of a laid-out pattern, in metres: polylines to stroke and circles `[x, y, r, filled]`. */
export interface PatternMarks {
  lines?: [number, number][][];
  circles?: [number, number, number, number][];
  tint?: boolean;
  /** Words along a line: centred at `at`, baseline turned `angle` radians in the path's own coordinates, `height` tall. */
  texts?: { at: [number, number]; angle: number; height: number; text: string }[];
}

/**
 * Writes a pattern's words. `px` maps a point of the marks' coordinates to the canvas, so the turn and size are worked out
 * from it (whichever way up those coordinates are).
 */
export function drawTexts(g: CanvasRenderingContext2D, texts: PatternMarks['texts'], px: (p: [number, number]) => [number, number]) {
  if (!texts?.length) return;
  g.save();
  g.fillStyle = g.strokeStyle;
  g.textAlign = 'center';
  g.textBaseline = 'middle';
  for (const t of texts) {
    const [x, y] = px(t.at);
    const along = px([t.at[0] + Math.cos(t.angle) * t.height, t.at[1] + Math.sin(t.angle) * t.height]);
    const h = Math.hypot(along[0] - x, along[1] - y);
    if (!(h >= 2)) continue; // too small to read at this zoom
    let a = Math.atan2(along[1] - y, along[0] - x);
    if (Math.cos(a) < -1e-6) a += Math.PI; // still never upside down on screen
    g.font = `${h.toFixed(1)}px system-ui, sans-serif`;
    g.save();
    g.translate(x, y);
    g.rotate(a);
    g.fillText(t.text, 0, 0);
    g.restore();
  }
  g.restore();
}

/** What a new part of a linetype starts as. */
export function newElement(kind: Element['t'], motif: string): Element {
  switch (kind) {
    case 'dash':
      return { t: 'dash', mm: 3 };
    case 'gap':
      return { t: 'gap', mm: 2 };
    case 'dot':
      return { t: 'dot' };
    case 'motif':
      return { t: 'motif', motif, size_mm: 2, turn_deg: 0, along_mm: 0, across_mm: 0 };
    case 'text':
      return { t: 'text', text: '?', size_mm: 2, turn_deg: 0, along_mm: 0, across_mm: 0 };
  }
}

/** What a new layer of a fill starts as. */
export function newLayer(kind: Layer['t'], motif: string): Layer {
  switch (kind) {
    case 'patlines':
      return { t: 'patlines', angle_deg: 45, x_mm: 0, y_mm: 0, shift_mm: 0, spacing_mm: 3, dashes_mm: [] };
    case 'hatch':
      return { t: 'hatch', angle_deg: 45, spacing_mm: 3 };
    case 'dots':
      return { t: 'dots', spacing_mm: 2.5, dot_mm: 0.35 };
    case 'tile':
      return { t: 'tile', motif, size_mm: 1.8, across_mm: 4, down_mm: 4, stagger: true, turn_deg: 0, random_turn: false, size_var: 0, jitter: 0, skip: 0 };
  }
}

/** A copy of a definition that is the cave's own: no `own` flag, so saving writes it. */
export function plain<T extends { own?: boolean }>(def: T): T {
  const { own: _own, ...rest } = def;
  return JSON.parse(JSON.stringify(rest)) as T; // not structuredClone: a reactive proxy cannot be cloned that way
}

/** A name for a copy that does not clash with `taken`: `Name copy`, `Name copy 2`, ... */
export function copyName(name: string, taken: string[]): string {
  const base = name.replace(/ copy( \d+)?$/, '');
  let n = taken.includes(`${base} copy`) ? 2 : 1;
  while (taken.includes(n === 1 ? `${base} copy` : `${base} copy ${n}`)) n++;
  return (n === 1 ? `${base} copy` : `${base} copy ${n}`).slice(0, 40);
}

/** Strokes laid-out marks. `px` turns a point in metres into the canvas pixel it lands on. Circles are `[x, y, r, filled]` in metres. */
export function drawMarks(g: CanvasRenderingContext2D, marks: PatternMarks, px: (p: [number, number]) => [number, number], pxPerMetre: number) {
  for (const line of marks.lines ?? []) {
    if (line.length < 2) continue;
    g.beginPath();
    const [x0, y0] = px(line[0]);
    g.moveTo(x0, y0);
    for (let i = 1; i < line.length; i++) {
      const [x, y] = px(line[i]);
      g.lineTo(x, y);
    }
    g.stroke();
  }
  for (const [x, y, r, filled] of marks.circles ?? []) {
    const [cx, cy] = px([x, y]);
    g.beginPath();
    g.arc(cx, cy, Math.max(r * pxPerMetre, 0.5), 0, Math.PI * 2);
    if (filled) g.fill();
    else g.stroke();
  }
  drawTexts(g, marks.texts, px);
}

/** Draws a motif's own drawing (unit coordinates) centred at `(cx, cy)`, `size` pixels across. */
export function drawMotif(g: CanvasRenderingContext2D, prims: Prim[], cx: number, cy: number, size: number) {
  const at = (p: [number, number]): [number, number] => [cx + p[0] * size, cy - p[1] * size];
  for (const p of prims) {
    if (p.t === 'circle') {
      const [x, y] = at(p.c);
      g.beginPath();
      g.arc(x, y, Math.max(p.r * size, 0.5), 0, Math.PI * 2);
      if (p.fill) g.fill();
      else g.stroke();
    } else if (p.pts.length >= 2) {
      g.beginPath();
      p.pts.forEach((q, i) => {
        const [x, y] = at(q);
        if (i) g.lineTo(x, y);
        else g.moveTo(x, y);
      });
      if (p.closed) g.closePath();
      if (p.fill && p.closed) g.fill();
      else g.stroke();
    }
  }
}

/** Thins a freehand stroke to the corners that matter (Ramer-Douglas-Peucker), so a drawn motif is a few points, not hundreds. */
export function simplify(points: [number, number][], epsilon: number): [number, number][] {
  if (points.length < 3) return points.slice();
  const keep = new Array<boolean>(points.length).fill(false);
  keep[0] = keep[points.length - 1] = true;
  const stack: [number, number][] = [[0, points.length - 1]];
  while (stack.length) {
    const [a, b] = stack.pop()!;
    let far = -1;
    let worst = epsilon;
    for (let i = a + 1; i < b; i++) {
      const d = distanceToSegment(points[i], points[a], points[b]);
      if (d > worst) {
        worst = d;
        far = i;
      }
    }
    if (far >= 0) {
      keep[far] = true;
      stack.push([a, far], [far, b]);
    }
  }
  return points.filter((_, i) => keep[i]);
}

function distanceToSegment(p: [number, number], a: [number, number], b: [number, number]): number {
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const len2 = dx * dx + dy * dy;
  const t = len2 ? Math.max(0, Math.min(1, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2)) : 0;
  return Math.hypot(p[0] - (a[0] + t * dx), p[1] - (a[1] + t * dy));
}
