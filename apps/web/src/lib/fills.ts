// Area fills (sand, clay, breakdown, water, flowstone, ...): a closed outline is stored; the marks
// that fill it are made by the engine from the outline and the stroke's id, the same on every device,
// and cached here until the outline changes. Nothing here touches the engine directly: it is
// installed once the engine has loaded.

export interface FillDef {
  key: string;
  label: string;
  help: string;
}

/** The marks that fill an outline, in the outline's own units. */
export interface Marks {
  lines: [number, number][][];
  circles: [number, number, number, number][];
  /** True when the whole outline is also tinted (water). */
  tint: boolean;
}

export interface FillSource {
  fills(): FillDef[];
  fillMarks(kind: string, curve: [number, number][], id: string, amount?: number, length?: number, angle?: number): Marks;
  /** A fill the caver authored (by name), at a drawing scale; empty for an unknown name. */
  fillPatternMarksNamed(name: string, outline: [number, number][], id: string, amount: number, scale: number, angle?: number): { lines?: [number, number][][]; circles?: [number, number, number, number][]; tint?: boolean };
  /** Goes up on every edit, so a changed pattern is not drawn from an old layout. */
  revision(): number;
}

let source: FillSource | null = null;
let defs: FillDef[] = [];

/** Installs the engine's fill catalogue (call once the engine has loaded). */
export function setFillSource(s: FillSource) {
  source = s;
  defs = s.fills();
}

/** True for a type that fills an area. */
export function isFillKind(kind: string | undefined): boolean {
  return !!kind && kind.startsWith('fill-');
}

/** Every kind of fill, in palette order. */
export function fillDefs(): FillDef[] {
  return defs;
}

/** The name people see for a fill type. */
export function fillLabel(kind: string): string | undefined {
  return defs.find((d) => d.key === kind)?.label;
}

/** The drawing scale the fills' authored sizes belong to (`cave_draw::BASE_SCALE`): `length` is the drawing's scale over this. */
const BASE_FILL_SCALE = 500;
const cache = new Map<string, Marks>();
const EMPTY: Marks = { lines: [], circles: [], tint: false };

/** The marks for a stroke: from the cache while its outline is unchanged. `curve` is the smooth outline. */
export function marksFor(kind: string, id: string, curve: [number, number][], amount = 1, length = 1, pattern = '', angle = 0): Marks {
  if (!source || curve.length < 3) return EMPTY;
  const custom = kind === 'fill-custom';
  if (custom && !pattern) return EMPTY; // a custom fill whose pattern is not named has nothing to draw yet
  const key = `${custom ? `${source.revision()}|${pattern}|` : ''}${kind}|${amount}|${length}|${angle}|${id}|${curve.length}|${curve[0]}|${curve[curve.length >> 1]}|${curve[curve.length - 1]}`;
  let m = cache.get(key);
  if (!m) {
    if (custom) {
      const r = source.fillPatternMarksNamed(pattern, curve, id, amount, length * BASE_FILL_SCALE, angle);
      m = { lines: r.lines ?? [], circles: r.circles ?? [], tint: !!r.tint };
    } else m = source.fillMarks(kind, curve, id, amount, length, angle);
    if (cache.size > 400) cache.clear(); // an editing session never needs more than a few at once
    cache.set(key, m);
  }
  return m;
}

/**
 * Draws a fill's marks with the context's stroke style. `to` maps the marks' units to pixels and
 * `outline` is the filled outline on screen (for the tint). Circle radii scale by the length of a
 * unit step along x.
 */
export function drawMarks(g: CanvasRenderingContext2D, marks: Marks, to: (x: number, y: number) => [number, number], outline: [number, number][]): void {
  if (marks.tint && outline.length > 2) {
    const alpha = g.globalAlpha;
    g.globalAlpha = alpha * 0.2;
    g.fillStyle = g.strokeStyle;
    g.beginPath();
    outline.forEach((p, i) => (i === 0 ? g.moveTo(p[0], p[1]) : g.lineTo(p[0], p[1])));
    g.closePath();
    g.fill();
    g.globalAlpha = alpha;
  }
  g.fillStyle = g.strokeStyle;
  for (const line of marks.lines) {
    g.beginPath();
    line.forEach((p, i) => {
      const [x, y] = to(p[0], p[1]);
      if (i === 0) g.moveTo(x, y);
      else g.lineTo(x, y);
    });
    g.stroke();
  }
  if (marks.circles.length) {
    const o = to(0, 0);
    const u = to(1, 0);
    const px = Math.hypot(u[0] - o[0], u[1] - o[1]); // pixels per unit
    for (const [x, y, r, filled] of marks.circles) {
      const [cx, cy] = to(x, y);
      g.beginPath();
      g.arc(cx, cy, Math.max(0.6, r * px), 0, Math.PI * 2);
      if (filled) g.fill();
      else g.stroke();
    }
  }
}
