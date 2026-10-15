// What `cave-snap` hands across (JSON): the shapes of its scene, settings, session and result. No logic here, only the
// types, the camera maths the browser needs to describe its view, and the preferences that are only about drawing.

export type SnapMode =
  | 'endpoint' | 'midpoint' | 'center' | 'geometric_center' | 'quadrant' | 'insertion' | 'node' | 'station'
  | 'intersection' | 'apparent_intersection' | 'extended_intersection' | 'extension' | 'perpendicular' | 'tangent'
  | 'parallel' | 'nearest' | 'tracking' | 'from' | 'mid_between' | 'track_point';

export type V3 = [number, number, number];
export interface SnapFrame {
  origin: V3;
  u: V3;
  v: V3;
}

export type SnapGeometry =
  | { type: 'polyline'; pts: [number, number][]; bulges?: number[]; closed?: boolean }
  | { type: 'spline'; pts: [number, number][]; closed?: boolean }
  | { type: 'circle'; center: [number, number]; radius: number }
  | { type: 'arc'; center: [number, number]; radius: number; start: number; sweep: number }
  | { type: 'point'; at: V3 };

export interface SnapThing {
  id: string;
  kind: 'line' | 'symbol' | 'text' | 'leader' | 'station' | 'node';
  frame: SnapFrame;
  geom: SnapGeometry;
  label?: string;
}

export interface SnapCamera {
  /** Column-major `projection * view`. */
  view_proj: number[];
  width: number;
  height: number;
}

export interface SnapScene {
  camera: SnapCamera;
  things: SnapThing[];
  plane: SnapFrame | null;
}

export interface SnapSettings {
  modes: SnapMode[];
  aperture: number;
  polar: number[];
  tracking: boolean;
  intersection_tol: number;
}

export type Continuation = { type: 'line'; dir: V3 } | { type: 'arc'; frame: SnapFrame; center: [number, number]; radius: number; angle: number; ccw: boolean };

export type SnapAcquired =
  | { type: 'extension'; at: V3; id: string; cont: Continuation }
  | { type: 'parallel'; dir: V3; id: string }
  | { type: 'track'; at: V3 };

export interface SnapSession {
  acquired?: SnapAcquired[];
  track_points?: V3[];
  from_base?: V3 | null;
  m2p_first?: V3 | null;
  last_point?: V3 | null;
  override?: SnapMode | null;
}

export interface SnapCandidate {
  mode: SnapMode;
  screen: [number, number];
  world: V3;
  source: { id: string; seg: number; t: number } | null;
  label: string;
  dist: number;
}

export interface SnapGuide {
  kind: 'extension' | 'track' | 'polar' | 'parallel';
  origin: V3;
  dir: V3 | null;
  px: [number, number][];
  label: string;
}

export interface SnapResult {
  best: SnapCandidate | null;
  others: SnapCandidate[];
  guides: SnapGuide[];
  acquirable: SnapAcquired[];
}

export interface SnapModeInfo {
  id: SnapMode;
  name: string;
  group: 'point' | 'relational' | 'reference';
  default: boolean;
}

export interface SnapCatalog {
  settings: SnapSettings;
  modes: SnapModeInfo[];
}

/** The outcome of a finished pick. */
export interface SnapOutcome {
  point: V3 | null;
  session: SnapSession;
}

/** What is only about how snapping looks and feels, not about what it finds. */
export interface SnapPrefs {
  marker: boolean;
  tooltip: boolean;
  magnet: boolean;
}

/** The frame of the horizontal plane at height `z` (the plan). */
export const planFrame = (z: number): SnapFrame => ({ origin: [0, 0, z], u: [1, 0, 0], v: [0, 1, 0] });

/** A camera from a three.js-style `projectionMatrix * matrixWorldInverse` (16 numbers, column-major) and the view's size. */
export const cameraOf = (viewProj: ArrayLike<number>, width: number, height: number): SnapCamera => ({ view_proj: Array.from(viewProj), width, height });

/**
 * The camera of a flat pane drawn with `px = tx + u * scale`, `py = ty - v * scale` (the sketchpad, the Profile and the
 * Cross Section): an orthographic view straight onto its plane.
 */
export function flatCamera(tf: { scale: number; tx: number; ty: number }, width: number, height: number): SnapCamera {
  const m = new Array<number>(16).fill(0);
  m[0] = (2 * tf.scale) / width;
  m[12] = (2 * tf.tx) / width - 1;
  m[5] = (2 * tf.scale) / height;
  m[13] = 1 - (2 * tf.ty) / height;
  m[10] = -0.001;
  m[15] = 1;
  return { view_proj: m, width, height };
}
