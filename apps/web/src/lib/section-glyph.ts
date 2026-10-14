// Where a station's cross-section sketches sit in the Map and Profile, and how they are hit-tested.
// A section is a small drawing in its own plane ([across, up] metres from its station); a view
// draws it at true size beside the station, joined to it by a dashed leader, and the person drags
// it to where they want it. The offset they leave it at is saved with the drawing.

import { drawing, fillLength, parseStyle, patternName, styleFor } from './line-style.svelte';
import { lineMarksPx, strokeMarksPx } from './pattern-draw';
import type { Look } from './ink';
import { smooth } from './spline.ts';
import { drawStamp, drawStampHalo, isStampKind, leaderLook, placement } from './symbols.ts';
import { drawMarks, isFillKind, marksFor } from './fills.ts';

/** Where shaped-line ornament comes from (the engine sets this once it has loaded). */
export interface OrnamentSource {
  ornament(kind: string, curve: [number, number][], closed: boolean, flip: boolean, id: string, survey: [number, number][]): { spine: boolean; lines: [number, number][][] };
  flippedLines(): string[];
}
let source: OrnamentSource | null = null;
/** Installs the ornament source. */
export function setOrnamentSource(s: OrnamentSource) {
  source = s;
}


/** The theme's highlight colour (what selection is drawn in). */
export function highlightColor(): string {
  if (typeof document === 'undefined') return '#2563eb';
  return getComputedStyle(document.documentElement).getPropertyValue('--accent').trim() || '#2563eb';
}

let hiddenTest: (kind: string) => boolean = () => false;
/** Installs the test for which types the person has hidden (the Show menu). */
export function setHiddenTest(f: (kind: string) => boolean) {
  hiddenTest = f;
}
/** True when strokes of this type are switched off in the Show menu. */
export function isKindHidden(kind: string): boolean {
  return hiddenTest(kind);
}

let flippedIds = new Set<string>();
/** Re-reads which shaped lines are flipped (call before drawing). */
export function refreshFlipped() {
  flippedIds = new Set(source?.flippedLines() ?? []);
}

const ornamentCache = new Map<string, { spine: boolean; lines: [number, number][][] }>();
const NONE = { spine: true, lines: [] as [number, number][][] };
const pointKey = (pts: [number, number][]) => (pts.length ? `${pts.length},${pts[0]},${pts[pts.length >> 1]},${pts[pts.length - 1]}` : '0');

/** The ornament of a shaped line: from the cache while the curve, the survey and the flip are unchanged. */
export function ornamentFor(kind: string, id: string, curve: [number, number][], closed: boolean, survey: [number, number][]): { spine: boolean; lines: [number, number][][] } {
  if (!source) return NONE;
  const flip = !!id && flippedIds.has(id);
  const key = `${kind}|${id}|${flip}|${closed}|${pointKey(curve)}|${kind === 'glyphs' ? pointKey(survey) : ''}`;
  let o = ornamentCache.get(key);
  if (!o) {
    o = source.ornament(kind, curve, closed, flip, id, survey);
    if (ornamentCache.size > 600) ornamentCache.clear();
    ornamentCache.set(key, o);
  }
  return o;
}

export type Sketches = { points: [number, number][]; widths: number[] | null; closed?: boolean; text?: string; style?: string }[];

/** `[minU, minV, maxU, maxV]` around the strokes (zeros when empty). */
export function sketchBounds(sketches: Sketches): [number, number, number, number] {
  let b: [number, number, number, number] | null = null;
  for (const k of sketches) {
    for (const [u, v] of k.points) {
      b = b ? [Math.min(b[0], u), Math.min(b[1], v), Math.max(b[2], u), Math.max(b[3], v)] : [u, v, u, v];
    }
  }
  return b ?? [0, 0, 0, 0];
}

/** Where an unplaced section sits in the map: just east of its station, clear of the strokes. */
export function defaultMapOffset(sketches: Sketches): [number, number, number] {
  const b = sketchBounds(sketches);
  return [Math.max(0, -b[0]) + 1.5, 0, 0];
}

/** Where an unplaced section sits on the band: just above its station, clear of the strokes. */
export function defaultBandOffset(sketches: Sketches): [number, number] {
  const b = sketchBounds(sketches);
  return [0, Math.max(0, -b[1]) + 2];
}

/** True when the screen point is on a section's strokes' box (grown by `pad` px). `px` is pixels per metre. */
export function hits(sketches: Sketches, anchor: [number, number], px: number, x: number, y: number, pad = 8): boolean {
  const b = sketchBounds(sketches);
  return x >= anchor[0] + b[0] * px - pad && x <= anchor[0] + b[2] * px + pad && y >= anchor[1] - b[3] * px - pad && y <= anchor[1] - b[1] * px + pad;
}

/** True when the stroke `id` is the selected one (or one of the selected ones). */
function isChosen(selected: string | ReadonlySet<string>, id: string | undefined): boolean {
  if (!id) return false;
  return typeof selected === 'string' ? !!selected && selected === id : selected.has(id);
}

/** Draws strokes as smooth curves through their fit points, in the look of their kind (width scaled
 *  by pen weight), `to` mapping section metres to pixels. The stroke with id `selected` gets a halo. */
export function drawStrokes(
  g: CanvasRenderingContext2D,
  sketches: (Sketches[number] & { kind: string; id?: string })[],
  to: (u: number, v: number) => [number, number],
  look: (kind: string, style?: string, pxPerM?: number) => Look,
  scale = 1,
  selected: string | ReadonlySet<string> = '',
  /** The survey's stations in the strokes' own coordinates (wall glyphs go on the side away from them). */
  survey: [number, number][] = [],
): void {
  g.lineCap = 'round';
  g.lineJoin = 'round';
  refreshFlipped(); // shaped lines with ornament on the other side
  const o = to(0, 0);
  const u = to(1, 0);
  const pxPerM = Math.hypot(u[0] - o[0], u[1] - o[1]); // how big a metre is here, for sizing patterns drawn on paper
  for (const k of sketches) {
    if (hiddenTest(k.kind)) continue; // switched off in the Show menu
    const style = styleFor(k.id, k.style); // what is being dragged for this line, or what it has
    const l = look(k.kind, style, pxPerM);
    if (isStampKind(k.kind) && k.points.length >= 2) {
      // A placed symbol: its first point is where it sits, its second says which way is up and how big.
      const p0 = to(k.points[0][0], k.points[0][1]);
      const p1 = to(k.points[1][0], k.points[1][1]);
      g.strokeStyle = l.color;
      g.globalAlpha = l.alpha;
      g.setLineDash([]);
      g.lineWidth = Math.max(1, l.weight * scale * 0.7);
      const look = leaderLook(style, (pxPerM * drawing.scale) / 1000);
      const more = k.points.slice(2).map((q) => to(q[0], q[1]));
      if (isChosen(selected, k.id) && (k.kind === 'callout' || k.kind === 'text')) {
        // a text or a leader: its lines thickened and a box round its words
        drawStampHalo(g, k.kind, p0, p1, (k as { text?: string }).text ?? '', look, more, highlightColor());
        g.setLineDash([]);
        g.lineWidth = Math.max(1, l.weight * scale * 0.7);
      } else if (isChosen(selected, k.id)) {
        const pl = placement(p0, p1);
        g.strokeStyle = highlightColor();
        g.globalAlpha = 0.9;
        g.lineWidth = 3;
        g.beginPath();
        g.arc(pl.cx, pl.cy, pl.r + 6, 0, Math.PI * 2);
        g.stroke();
        g.strokeStyle = l.color;
        g.globalAlpha = l.alpha;
        g.lineWidth = Math.max(1, l.weight * scale * 0.7);
      }
      drawStamp(g, k.kind, p0, p1, (k as { text?: string }).text, look, more);
      continue;
    }
    const curve = smooth(
      k.points.map((p) => to(p[0], p[1])),
      k.widths,
      10,
      !!k.closed,
    );
    // Shaped lines: ornament is regenerated from the curve (in metres) every time it is drawn.
    const metres = smooth(k.points, null, 10, !!k.closed).pts;
    const orn = ornamentFor(k.kind, k.id ?? '', metres, !!k.closed, survey);
    const trace = (extra: number, color: string, alpha: number, dash: number[]) => {
      g.strokeStyle = color;
      g.globalAlpha = alpha;
      g.setLineDash(dash);
      for (let i = 1; i < curve.pts.length; i++) {
        g.lineWidth = l.weight * scale * (curve.widths ? (curve.widths[i - 1] + curve.widths[i]) / 2 : 1) + extra;
        g.beginPath();
        g.moveTo(curve.pts[i - 1][0], curve.pts[i - 1][1]);
        g.lineTo(curve.pts[i][0], curve.pts[i][1]);
        g.stroke();
      }
    };
    if (isChosen(selected, k.id)) {
      trace(9, highlightColor(), 0.6, []);
      if (isFillKind(k.kind) && curve.pts.length >= 3) {
        // a selected area is tinted all through, not only round its edge
        g.save();
        g.globalAlpha = 0.16;
        g.fillStyle = highlightColor();
        g.beginPath();
        curve.pts.forEach((q, i) => (i === 0 ? g.moveTo(q[0], q[1]) : g.lineTo(q[0], q[1])));
        g.closePath();
        g.fill();
        g.restore();
      }
    }
    if (isFillKind(k.kind) && k.points.length >= 3) {
      // An area fill: its marks are made from the outline (in metres) and the stroke's id, then drawn under the outline.
      g.strokeStyle = l.color;
      g.globalAlpha = l.alpha;
      g.setLineDash([]);
      g.lineWidth = Math.max(1, l.weight * scale * 0.7);
      drawMarks(g, marksFor(k.kind, k.id ?? '', metres, parseStyle(style).density, fillLength(), patternName(parseStyle(style).lt), parseStyle(style).angle), to, curve.pts);
    }
    if (orn.spine && l.custom && curve.pts.length >= 2) {
      // the caver's own linetype: dashes, dots and motifs laid along the curve in pixels
      g.strokeStyle = l.color;
      g.fillStyle = l.color;
      g.globalAlpha = l.alpha;
      g.setLineDash([]);
      g.lineWidth = Math.max(1, l.weight * scale);
      strokeMarksPx(g, lineMarksPx(l.custom, curve.pts as [number, number][], l.pxPerMm ?? 0));
    } else if (orn.spine) trace(0, l.color, l.alpha, l.dash);
    if (orn.lines.length) {
      g.strokeStyle = l.color;
      g.globalAlpha = l.alpha;
      g.lineWidth = l.weight * scale * 0.8;
      for (const line of orn.lines) {
        g.beginPath();
        line.forEach((p, i) => {
          const [x, y] = to(p[0], p[1]);
          if (i === 0) g.moveTo(x, y);
          else g.lineTo(x, y);
        });
        g.stroke();
      }
    }
  }
  g.setLineDash([]);
  g.globalAlpha = 1;
}

/** Draws the fit points of a selected stroke as handles that can be picked up. */
export function drawHandles(g: CanvasRenderingContext2D, pts: [number, number][], color: string, bg: string): void {
  g.lineWidth = 1.5;
  for (const p of pts) {
    g.fillStyle = bg;
    g.strokeStyle = color;
    g.beginPath();
    g.rect(p[0] - 4.5, p[1] - 4.5, 9, 9);
    g.fill();
    g.stroke();
  }
}

/** The smooth curves of a section's strokes on screen (what is actually drawn). */
export function glyphCurves(sketches: Sketches, to: (u: number, v: number) => [number, number]): [number, number][][] {
  return sketches.map((k) => smooth(k.points.map((p) => to(p[0], p[1])), null, 10, !!k.closed).pts);
}

/**
 * Where a leader from the station should stop: on the outer shell of the drawing. It runs from the
 * station toward the middle of the sketch and ends at the first stroke it meets, so it never
 * crosses into the drawing; if it would miss every stroke (a gap), it ends at the stroke nearest
 * the station. Also returns the drawing's middle and a spot just under it for the label.
 */
export function leaderEnd(from: [number, number], curves: [number, number][][]): { end: [number, number]; centre: [number, number]; below: [number, number] } {
  const all = curves.flat();
  if (all.length === 0) return { end: from, centre: from, below: from };
  const xs = all.map((p) => p[0]);
  const ys = all.map((p) => p[1]);
  const centre: [number, number] = [(Math.min(...xs) + Math.max(...xs)) / 2, (Math.min(...ys) + Math.max(...ys)) / 2];
  const below: [number, number] = [centre[0], Math.max(...ys) + 13];
  const dx = centre[0] - from[0];
  const dy = centre[1] - from[1];
  let bestT = Infinity;
  let nearest: [number, number] = all[0];
  let nearestD = Infinity;
  for (const c of curves) {
    for (let i = 1; i < c.length; i++) {
      const [a, b] = [c[i - 1], c[i]];
      const ex = b[0] - a[0];
      const ey = b[1] - a[1];
      // where the leader (from + t * d) crosses this piece of stroke (a + u * e)
      const det = dx * ey - dy * ex;
      if (Math.abs(det) > 1e-9) {
        const t = ((a[0] - from[0]) * ey - (a[1] - from[1]) * ex) / det;
        const u = ((a[0] - from[0]) * dy - (a[1] - from[1]) * dx) / det;
        if (t >= 0 && t <= 1 && u >= 0 && u <= 1) bestT = Math.min(bestT, t);
      }
      // the point of this piece nearest the station, in case the leader misses every stroke
      const len2 = ex * ex + ey * ey;
      const u = len2 < 1e-12 ? 0 : Math.max(0, Math.min(1, ((from[0] - a[0]) * ex + (from[1] - a[1]) * ey) / len2));
      const q: [number, number] = [a[0] + u * ex, a[1] + u * ey];
      const d = Math.hypot(q[0] - from[0], q[1] - from[1]);
      if (d < nearestD) {
        nearestD = d;
        nearest = q;
      }
    }
  }
  const end: [number, number] = Number.isFinite(bestT) ? [from[0] + dx * bestT, from[1] + dy * bestT] : nearest;
  return { end, centre, below };
}
