// Draws a laid-out sheet on a canvas: white paper, the furniture the engine made, and the map (the
// passage walls, the survey legs and everything drawn on the map) at the sheet's scale. The same
// function draws the screen preview and the exported picture, so what you see is what you print.

import type { Scene, SheetPlan, ProfileSketch3d, RenderedSheet, PlacedFrame, SheetShape } from './engine';
import { drawStrokes } from './section-glyph';
import type { Look } from './ink';

const FT_PER_M = 1 / 0.3048;

/** Where the cave sits on the sheet: its middle at the middle of the map box. Metres to inches from the paper's lower left. */
export function mapTransform(plan: SheetPlan, bounds: [number, number, number, number, number, number] | null): (x: number, y: number) => [number, number] {
  const k = FT_PER_M / plan.feet_per_inch; // inches of paper per metre of cave
  const cx = bounds ? (bounds[0] + bounds[3]) / 2 : 0;
  const cy = bounds ? (bounds[1] + bounds[4]) / 2 : 0;
  const mx = (plan.map[0] + plan.map[2]) / 2;
  const my = (plan.map[1] + plan.map[3]) / 2;
  return (x, y) => [mx + (x - cx) * k, my + (y - cy) * k];
}

/** True when the cave, at the sheet's scale, is bigger than the box the map may use. */
export function overflows(plan: SheetPlan, bounds: [number, number, number, number, number, number] | null): boolean {
  if (!bounds) return false;
  const k = FT_PER_M / plan.feet_per_inch;
  return (bounds[3] - bounds[0]) * k > plan.map[2] - plan.map[0] + 1e-6 || (bounds[4] - bounds[1]) * k > plan.map[3] - plan.map[1] + 1e-6;
}

/** Paints the sheet at `px` pixels per inch of paper onto a canvas already sized to the paper. */
export function drawSheet(g: CanvasRenderingContext2D, plan: SheetPlan, scene: Scene | null, lines: ProfileSketch3d[], px: number, look: (kind: string, style?: string, pxPerM?: number) => Look): void {
  const H = plan.h * px;
  const P = (p: [number, number]): [number, number] => [p[0] * px, H - p[1] * px];
  g.save();
  g.fillStyle = '#ffffff';
  g.fillRect(0, 0, plan.w * px, H);
  const ink = '#111111';
  g.strokeStyle = ink;
  g.fillStyle = ink;
  g.lineCap = 'round';
  g.lineJoin = 'round';
  // the map, clipped to its box
  const to = mapTransform(plan, scene?.bounds ?? null);
  const toPx = (x: number, y: number) => P(to(x, y));
  g.save();
  g.beginPath();
  g.rect(plan.map[0] * px, H - plan.map[3] * px, (plan.map[2] - plan.map[0]) * px, (plan.map[3] - plan.map[1]) * px);
  g.clip();
  const w = Math.max(1, px / 60);
  if (scene) {
    g.lineWidth = w * 1.4;
    for (const wall of scene.walls) {
      g.beginPath();
      wall.forEach((p, i) => {
        const [x, y] = toPx(p[0], p[1]);
        if (i === 0) g.moveTo(x, y);
        else g.lineTo(x, y);
      });
      g.stroke();
    }
    g.lineWidth = w * 0.6;
    g.globalAlpha = 0.55;
    for (const l of scene.legs) {
      const [a, b] = [toPx(l.from[0], l.from[1]), toPx(l.to[0], l.to[1])];
      g.beginPath();
      g.moveTo(a[0], a[1]);
      g.lineTo(b[0], b[1]);
      g.stroke();
    }
    g.globalAlpha = 1;
  }
  const printLook = (kind: string, style?: string, pxPerM?: number): Look => ({ ...look(kind, style, pxPerM), color: ink });
  drawStrokes(
    g,
    lines.map((l) => ({ id: l.id, kind: l.kind, points: l.points.map((p) => [p[0], p[1]] as [number, number]), widths: l.widths, closed: l.closed, text: l.text })),
    toPx,
    printLook,
    Math.max(1, px / 60),
    '',
    (scene?.stations ?? []).map((s) => [s.at[0], s.at[1]] as [number, number]),
  );
  g.restore();
  // the furniture
  drawShapes(g, plan.shapes, P, px);
  g.restore();
}

/** Where a map point (metres) lands on the paper (inches from the lower left) through a viewframe: its centre at the middle of
 *  the frame's box, turned by the frame's twist (degrees counter-clockwise), `k` inches of paper per metre. */
export function frameTransform(f: PlacedFrame): (x: number, y: number) => [number, number] {
  const mx = (f.rect[0] + f.rect[2]) / 2;
  const my = (f.rect[1] + f.rect[3]) / 2;
  const t = (f.twist * Math.PI) / 180;
  const c = Math.cos(t);
  const sn = Math.sin(t);
  return (x, y) => {
    const dx = x - f.center[0];
    const dy = y - f.center[1];
    return [mx + f.k * (c * dx - sn * dy), my + f.k * (sn * dx + c * dy)];
  };
}

/**
 * Draws a stored sheet: white paper, the live map seen through each of its viewframes (clipped to the frame,
 * at the frame's own scale and turn, with Ink style, fills and everything drawn on the map), then the
 * furniture. `px` is pixels per inch of paper. `chosen` outlines one frame (the one being worked on).
 */
export function drawStoredSheet(g: CanvasRenderingContext2D, sheet: RenderedSheet, scene: Scene | null, lines: ProfileSketch3d[], px: number, look: (kind: string, style?: string, pxPerM?: number) => Look, chosen = ''): void {
  const H = sheet.h * px;
  const P = (p: [number, number]): [number, number] => [p[0] * px, H - p[1] * px];
  g.save();
  g.fillStyle = '#ffffff';
  g.fillRect(0, 0, sheet.w * px, H);
  const ink = '#111111';
  g.strokeStyle = ink;
  g.fillStyle = ink;
  g.lineCap = 'round';
  g.lineJoin = 'round';
  const printLook = (kind: string, style?: string, pxPerM?: number): Look => ({ ...look(kind, style, pxPerM), color: ink });
  for (const f of sheet.frames) {
    const to = frameTransform(f);
    const toPx = (x: number, y: number) => P(to(x, y));
    g.save();
    g.beginPath();
    if (f.clip.length >= 3) f.clip.forEach((q, i) => (i === 0 ? g.moveTo(...P(q)) : g.lineTo(...P(q))));
    else g.rect(f.rect[0] * px, H - f.rect[3] * px, (f.rect[2] - f.rect[0]) * px, (f.rect[3] - f.rect[1]) * px);
    g.closePath();
    g.clip();
    const w = Math.max(1, px / 60);
    const hidden = new Set(f.hidden);
    if (scene) {
      if (!hidden.has('wall')) {
        g.lineWidth = w * 1.4;
        for (const wall of scene.walls) {
          g.beginPath();
          wall.forEach((p, i) => (i === 0 ? g.moveTo(...toPx(p[0], p[1])) : g.lineTo(...toPx(p[0], p[1]))));
          g.stroke();
        }
      }
      if (!hidden.has('centerline')) {
        g.lineWidth = w * 0.6;
        g.globalAlpha = 0.55;
        for (const l of scene.legs) {
          g.beginPath();
          g.moveTo(...toPx(l.from[0], l.from[1]));
          g.lineTo(...toPx(l.to[0], l.to[1]));
          g.stroke();
        }
        g.globalAlpha = 1;
      }
    }
    drawStrokes(
      g,
      lines.filter((l) => !hidden.has(l.kind)).map((l) => ({ id: l.id, kind: l.kind, points: l.points.map((p) => [p[0], p[1]] as [number, number]), widths: l.widths, closed: l.closed, text: l.text, style: l.style })),
      toPx,
      printLook,
      w,
      '',
      (scene?.stations ?? []).map((s) => [s.at[0], s.at[1]] as [number, number]),
    );
    g.restore();
    // the frame's own border, and its label above it
    g.lineWidth = Math.max(1, px / 90);
    g.strokeStyle = f.id === chosen ? '#188cff' : ink;
    g.strokeRect(f.rect[0] * px, H - f.rect[3] * px, (f.rect[2] - f.rect[0]) * px, (f.rect[3] - f.rect[1]) * px);
    g.strokeStyle = ink;
    if (f.label) {
      g.font = `${0.14 * px}px system-ui, sans-serif`;
      g.fillText(f.label, f.rect[0] * px, H - f.rect[3] * px - 0.06 * px);
    }
  }
  drawShapes(g, sheet.shapes, P, px);
  g.restore();
}

/** The sheet's furniture (title block, scale bar, north arrow, match lines), in inches. */
function drawShapes(g: CanvasRenderingContext2D, shapes: SheetShape[], P: (p: [number, number]) => [number, number], px: number) {
  g.lineWidth = Math.max(1, px / 100);
  for (const s of shapes) {
    if (s.t === 'line' || s.t === 'dash') {
      g.setLineDash(s.t === 'dash' ? [px * 0.08, px * 0.06] : []);
      g.beginPath();
      g.moveTo(...P(s.a));
      g.lineTo(...P(s.b));
      g.stroke();
      g.setLineDash([]);
    } else if (s.t === 'rect' || s.t === 'back') {
      const [a, b] = [P(s.a), P(s.b)];
      if (s.t === 'back') {
        g.save();
        g.fillStyle = '#ffffff';
        g.fillRect(a[0], b[1], b[0] - a[0], a[1] - b[1]);
        g.restore();
      } else g.strokeRect(a[0], b[1], b[0] - a[0], a[1] - b[1]);
    } else if (s.t === 'fill') {
      g.beginPath();
      s.pts.forEach((q, i) => (i === 0 ? g.moveTo(...P(q)) : g.lineTo(...P(q))));
      g.closePath();
      g.fill();
    } else {
      const [x, y] = P(s.at);
      g.font = `${s.h * px}px system-ui, sans-serif`;
      g.textBaseline = 'alphabetic';
      if (s.t === 'turned') {
        g.save();
        g.translate(x, y);
        g.rotate((-s.deg * Math.PI) / 180);
        g.fillText(s.s, 0, 0);
        g.restore();
      } else g.fillText(s.s, x, y);
    }
  }
}
