// The library of arrowheads and note frames a leader can use. Each is plain geometry: an arrowhead in its own unit space
// (tip at the origin, pointing along +x, the body of the arrow running back along -x), a frame as the outline of the note's
// box. Nothing here touches the engine or the screen state, so it is checked on its own (`tests/shapes.check.ts`).

export type Pt = [number, number];

/** One piece of an arrowhead: a polyline, closed or not, drawn filled, stroked or both. */
export interface HeadPart {
  pts: Pt[];
  closed?: boolean;
  fill?: boolean;
  stroke?: boolean;
}

/** The arrowheads drawn from the library (the engine draws the plain open and filled ones; see `drawStamp`). */
export const ARROWHEADS: Record<string, HeadPart[]> = {
  closed: [{ pts: [[0, 0], [-1, 0.36], [-1, -0.36]], closed: true, stroke: true }],
  stealth: [{ pts: [[0, 0], [-1, 0.4], [-0.7, 0], [-1, -0.4]], closed: true, fill: true, stroke: true }],
  diamond: [{ pts: [[0, 0], [-0.5, 0.3], [-1, 0], [-0.5, -0.3]], closed: true, fill: true, stroke: true }],
  'open-diamond': [{ pts: [[0, 0], [-0.5, 0.3], [-1, 0], [-0.5, -0.3]], closed: true, stroke: true }],
  square: [{ pts: [[0.2, 0.3], [-0.4, 0.3], [-0.4, -0.3], [0.2, -0.3]], closed: true, fill: true, stroke: true }],
  tick: [{ pts: [[0.3, 0.5], [-0.3, -0.5]], stroke: true }],
  bar: [{ pts: [[0, 0.5], [0, -0.5]], stroke: true }],
  double: [
    { pts: [[0, 0], [-0.8, 0.4]], stroke: true },
    { pts: [[0, 0], [-0.8, -0.4]], stroke: true },
    { pts: [[-0.55, 0.25], [-1.35, 0.65]], stroke: true },
    { pts: [[-0.55, -0.25], [-1.35, -0.65]], stroke: true },
  ],
};

/** True for an arrowhead the engine already lays out (the usual open one, filled, none, and the dot at the tip). */
export const ENGINE_HEADS = ['', 'arrow', 'filled', 'none', 'dot'];

/** An arrowhead's pieces placed at a tip: `scale` is its length, `dir` the way the arrow points (a unit vector). Null if the name is not in the library. */
export function placeHead(name: string, tip: Pt, dir: Pt, scale: number): HeadPart[] | null {
  const parts = ARROWHEADS[name];
  if (!parts) return null;
  const at = ([x, y]: Pt): Pt => [tip[0] + (x * dir[0] - y * dir[1]) * scale, tip[1] + (x * dir[1] + y * dir[0]) * scale];
  return parts.map((p) => ({ ...p, pts: p.pts.map(at) }));
}

/**
 * The outline of a note's frame round the box `[x1, top, x2, bottom]` with `pad` of room, as a closed polygon (a curve
 * is sampled finely). `underline` is the one open line and comes back with `closed: false`. Null for no frame.
 */
export function frameOutline(frame: string, box: [number, number, number, number], pad: number): { pts: Pt[]; closed: boolean } | null {
  const [x1, top, x2, bottom] = box;
  const l = x1 - pad;
  const r = x2 + pad;
  const t = top - pad;
  const b = bottom + pad;
  const cx = (l + r) / 2;
  const cy = (t + b) / 2;
  const w = r - l;
  const h = b - t;
  switch (frame) {
    case 'box':
      return { pts: [[l, t], [r, t], [r, b], [l, b]], closed: true };
    case 'underline':
      return { pts: [[l, bottom + pad * 0.5], [r, bottom + pad * 0.5]], closed: false };
    case 'rounded': {
      const k = Math.min(w, h) * 0.3;
      const pts: Pt[] = [];
      const corner = (ox: number, oy: number, a0: number) => {
        for (let i = 0; i <= 6; i++) {
          const a = a0 + (i / 6) * (Math.PI / 2);
          pts.push([ox + k * Math.cos(a), oy + k * Math.sin(a)]);
        }
      };
      corner(r - k, t + k, -Math.PI / 2);
      corner(r - k, b - k, 0);
      corner(l + k, b - k, Math.PI / 2);
      corner(l + k, t + k, Math.PI);
      return { pts, closed: true };
    }
    case 'hexagon': {
      const d = Math.min(h * 0.5, w * 0.25);
      return { pts: [[l + d, t], [r - d, t], [r, cy], [r - d, b], [l + d, b], [l, cy]], closed: true };
    }
    case 'octagon': {
      const d = Math.min(w, h) * 0.3;
      return { pts: [[l + d, t], [r - d, t], [r, t + d], [r, b - d], [r - d, b], [l + d, b], [l, b - d], [l, t + d]], closed: true };
    }
    case 'diamond':
      return { pts: [[cx, t - h * 0.2], [r + w * 0.12, cy], [cx, b + h * 0.2], [l - w * 0.12, cy]], closed: true };
    case 'ellipse': {
      const pts: Pt[] = [];
      for (let i = 0; i < 40; i++) {
        const a = (i / 40) * Math.PI * 2;
        pts.push([cx + (w / 2) * Math.SQRT2 * Math.cos(a), cy + (h / 2) * Math.SQRT2 * Math.sin(a)]);
      }
      return { pts, closed: true };
    }
    case 'banner': {
      const d = h * 0.35;
      return { pts: [[l, t], [r, t], [r - d, cy], [r, b], [l, b], [l + d, cy]], closed: true };
    }
    default:
      return null;
  }
}
