// Draws a laid-out sheet on a canvas: white paper, the furniture the engine made, and the map (the
// passage walls, the survey legs and everything drawn on the map) at the sheet's scale. The same
// function draws the screen preview and the exported picture, so what you see is what you print.

import type { Scene, SheetPlan, ProfileSketch3d } from './engine';
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
  g.lineWidth = Math.max(1, px / 100);
  g.globalAlpha = 1;
  for (const s of plan.shapes) {
    if (s.t === 'line') {
      const [a, b] = [P(s.a), P(s.b)];
      g.beginPath();
      g.moveTo(a[0], a[1]);
      g.lineTo(b[0], b[1]);
      g.stroke();
    } else if (s.t === 'rect') {
      const [a, b] = [P(s.a), P(s.b)];
      g.strokeRect(a[0], b[1], b[0] - a[0], a[1] - b[1]);
    } else if (s.t === 'fill') {
      g.beginPath();
      s.pts.forEach((q, i) => {
        const [x, y] = P(q);
        if (i === 0) g.moveTo(x, y);
        else g.lineTo(x, y);
      });
      g.closePath();
      g.fill();
    } else {
      const [x, y] = P(s.at);
      g.font = `${s.h * px}px system-ui, sans-serif`;
      g.textBaseline = 'alphabetic';
      g.fillText(s.s, x, y);
    }
  }
  g.restore();
}
