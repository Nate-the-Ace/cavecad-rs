// What `cave-sketch` hands across (JSON): the survey as shots and passage dimensions, and the views made of it.
// No logic lives here, only the types and the small canvas maths a pane needs every frame.

export type Pt = [number, number];

export interface SurveyShot {
  id: string;
  from: string;
  to: string | null;
  distance: number | null;
  az: number | null;
  inc: number | null;
  azBack: number | null;
  incBack: number | null;
}
export interface Dims {
  left: number | null;
  right: number | null;
  up: number | null;
  down: number | null;
}
export type DimsByStation = Record<string, Dims>;
export interface SketchSource {
  shots: SurveyShot[];
  dims: DimsByStation;
}

export interface Mark {
  label: string;
  p: Pt;
}
export interface Layout {
  stations: { label: string; x: number; y: number; z: number }[];
  legs: { id: string; from: string; to: string }[];
  splays: { id: string; from: string; end: [number, number, number] }[];
  /** The longest run of stations from the first, the spine of the profile. */
  chain: string[];
}
export interface PlanView {
  stations: Mark[];
  legs: [Pt, Pt][];
  splays: [Pt, Pt][];
  walls: Pt[][];
  passage: Pt[][];
}
export interface ProfileView {
  stations: Mark[];
  legs: [Pt, Pt][];
  walls: Pt[][];
  passage: Pt[][];
}
export interface SectionView {
  station: string;
  outline: Pt[] | null;
  splays: Pt[];
}
export interface SketchViews {
  layout: Layout;
  plan: PlanView;
  profile: ProfileView;
  section: SectionView | null;
}

/** A stroke as stored: its fit points relative to the nearest station of the view it was drawn in. */
export interface Anchored {
  anchor: string;
  pts: Pt[];
  widths: number[] | null;
  kind: string;
}

// ---- fitting a view to a pane (canvas maths, per frame) ----

export interface Transform {
  /** Pixels per metre. */
  scale: number;
  /** Where the view's origin lands on the pane. */
  tx: number;
  ty: number;
}

export function bounds(points: Pt[]): { x0: number; y0: number; x1: number; y1: number } | null {
  if (points.length === 0) return null;
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const [x, y] of points) {
    x0 = Math.min(x0, x);
    y0 = Math.min(y0, y);
    x1 = Math.max(x1, x);
    y1 = Math.max(y1, y);
  }
  return { x0, y0, x1, y1 };
}

/** A transform that shows `points` whole in a pane of `w` by `h` pixels, true to scale, with `pad` pixels round it and at most `maxScale` pixels per metre. */
export function fitView(points: Pt[], w: number, h: number, pad = 28, maxScale = 80): Transform {
  const b = bounds(points);
  if (!b || w <= 2 * pad || h <= 2 * pad) return { scale: 20, tx: w / 2, ty: h / 2 };
  const spanX = Math.max(b.x1 - b.x0, 1);
  const spanY = Math.max(b.y1 - b.y0, 1);
  const scale = Math.min((w - 2 * pad) / spanX, (h - 2 * pad) / spanY, maxScale);
  const cx = (b.x0 + b.x1) / 2;
  const cy = (b.y0 + b.y1) / 2;
  return { scale, tx: w / 2 - cx * scale, ty: h / 2 + cy * scale };
}

/** World metres to pane pixels (y flips: up on the page is up in the world). */
export const toScreen = (t: Transform, p: Pt): Pt => [t.tx + p[0] * t.scale, t.ty - p[1] * t.scale];
export const toWorld = (t: Transform, p: Pt): Pt => [(p[0] - t.tx) / t.scale, (t.ty - p[1]) / t.scale];
