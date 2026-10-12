// Loads the Rust engine (WASM) once and re-exports the few calls the UI uses.
import init, { start, log, console_events, console_clear, engine_version, open_demo, scene, profile_runs, profile_band, add_profile_sketch, undo, redo, edit_state, passage_mesh, shading } from '../wasm/cave_wasm.js';

export type Level = 'debug' | 'detail' | 'info' | 'warn' | 'error';
export interface ConsoleEvent {
  seq: number;
  level: Level;
  source: string;
  message: string;
}
export type Pt = [number, number, number];
export interface Scene {
  stations: { label: string; at: Pt; anchor: boolean }[];
  legs: { from: Pt; to: Pt; style: 'new' | 'closure' | 'tie' }[];
  /** [x1, y1, z1, x2, y2, z2] */
  splays: [number, number, number, number, number, number][];
  /** Passage wall lines (both sides); each is a run of 3D points. */
  walls: Pt[][];
  shading: {
    splays_over: number[][];
    splays_under: number[][];
    walls_over: number[][];
    walls_under: number[][];
    legs_over: number[][];
    legs_under: number[][];
  };
  closures: { at: Pt; horizontal: number; vertical: number; percent: number | null }[];
  /** [minX, minY, minZ, maxX, maxY, maxZ] */
  bounds: [number, number, number, number, number, number] | null;
  adjusted: boolean;
  skipped: number;
}
export interface RunInfo {
  key: string;
  stations: number;
  length: number;
  tie: string | null;
}
export interface ProfileScene {
  key: string;
  tie: string | null;
  stations: { label: string; s: number; z: number; up: number | null; down: number | null }[];
  /** [s1, z1, s2, z2] */
  legs: [number, number, number, number][];
  ceiling: [number, number][][];
  floor: [number, number][][];
  sketches: { id: string; points: [number, number][]; widths: number[] | null }[];
  /** Sketches that cannot be placed because a station they hang on is not on this band. */
  unplaced: number;
  omitted: number;
  note: string | null;
  /** [minS, minZ, maxS, maxZ] */
  bounds: [number, number, number, number] | null;
}
/** Flat triangle buffers for the passage surface (vertices are drawn in order). */
export interface PassageBuffers {
  positions: Float32Array;
  normals: Float32Array;
  colors: Float32Array;
  /** Unmeasured stations given an inferred (paler) section. */
  filled: number;
}
export interface BuildInfo {
  builtAt: string | null;
  git: string;
  dirty: boolean;
  error: string | null;
}

let ready: Promise<void> | null = null;

export function loadEngine(): Promise<void> {
  ready ??= init().then(() => start());
  return ready;
}

export const engine = {
  version: () => engine_version(),
  log: (level: Level, source: string, message: string) => log(level, source, message),
  events: (since: number): ConsoleEvent[] => JSON.parse(console_events(BigInt(since))),
  clear: () => console_clear(),
  openDemo: (json: string): string => open_demo(json),
  scene: (adjusted: boolean): Scene => JSON.parse(scene(adjusted)),
  /** Which drawn lines run under another passage, for a camera looking from `dir` toward the cave. */
  shading: (adjusted: boolean, dir: [number, number, number]): Scene['shading'] => JSON.parse(shading(adjusted, dir[0], dir[1], dir[2])),
  profileRuns: (adjusted: boolean): RunInfo[] => JSON.parse(profile_runs(adjusted)),
  profileBand: (run: string, adjusted: boolean): ProfileScene | null => JSON.parse(profile_band(run, adjusted)),
  /** Adds a stroke drawn on a run's band; every edit goes through the document's one edit path. */
  addSketch: (run: string, points: [number, number][], widths: number[] | null, adjusted: boolean): string => add_profile_sketch(run, JSON.stringify(points), widths ? JSON.stringify(widths) : '', adjusted),
  passageMesh: (adjusted: boolean, mode: 'trip' | 'depth', fillGaps: boolean): PassageBuffers => {
    const m = passage_mesh(adjusted, mode, fillGaps);
    const out = { positions: m.positions, normals: m.normals, colors: m.colors, filled: m.filled };
    m.free();
    return out;
  },
  undo: (): boolean => undo(),
  redo: (): boolean => redo(),
  editState: (): { undo: boolean; redo: boolean } => JSON.parse(edit_state()),
};

/** What build is this page running? Always fetched fresh. */
export async function loadBuildInfo(): Promise<BuildInfo | null> {
  try {
    const r = await fetch('/build-info.json', { cache: 'no-store' });
    return r.ok ? await r.json() : null;
  } catch {
    return null;
  }
}

// Dev-only handle for tests and debugging in the browser console.
if (import.meta.env.DEV) (window as unknown as { __engine: typeof engine }).__engine = engine;
