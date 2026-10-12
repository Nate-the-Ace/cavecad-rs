// Point symbols (stalactites, pits, springs, ...). A placed symbol is a stroke of type `sym-<name>`
// with two points: where it sits, and a second point that says which way is up for the symbol
// (its direction) and how big it is (its distance from the first point, as the symbol's radius).
// Drawing is plain lines and circles in the symbol's own unit space, so every canvas draws them
// the same way. Nothing here touches the engine: the catalogue is installed once it has loaded.

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

/** True for a type that is placed by a click and a drag rather than drawn: a symbol, or text. */
export function isStampKind(kind: string | undefined): boolean {
  return isSymbolKind(kind) || kind === 'text';
}

/** Where a stamp's words go and how big: for text, at the first point, turned to the aim; for a symbol's label, to its right. */
function wordsPlacement(kind: string, pl: ReturnType<typeof placement>): { x: number; y: number; size: number; angle: number } {
  if (kind === 'text') return { x: pl.cx, y: pl.cy, size: Math.max(6, pl.r), angle: Math.atan2(pl.ux, -pl.uy) };
  return { x: pl.cx + 1.3 * pl.r, y: pl.cy, size: Math.max(7, pl.r * 0.9), angle: 0 };
}

/** Draws a symbol (with its label) or a text stroke at `p0`, aimed toward `p1`. */
export function drawStamp(g: CanvasRenderingContext2D, kind: string, p0: [number, number], p1: [number, number], text = ''): void {
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
export function hitsStamp(kind: string, text: string | undefined, p0: [number, number], p1: [number, number], x: number, y: number, pad = 4): boolean {
  const pl = placement(p0, p1);
  if (!text) return hitsSymbol(p0, p1, x, y, pad);
  const w = wordsPlacement(kind, pl);
  // the words' box in their own frame: along the baseline and across it
  const dx = x - w.x;
  const dy = y - w.y;
  const along = dx * Math.cos(w.angle) + dy * Math.sin(w.angle);
  const across = -dx * Math.sin(w.angle) + dy * Math.cos(w.angle);
  const inWords = along >= -pad && along <= text.length * w.size * 0.6 + pad && Math.abs(across) <= w.size * 0.6 + pad;
  return inWords || (kind !== 'text' && hitsSymbol(p0, p1, x, y, pad));
}
