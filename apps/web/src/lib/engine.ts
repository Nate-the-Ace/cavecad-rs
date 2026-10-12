// Loads the Rust engine (WASM) once and re-exports the few calls the UI uses.
import init, { start, log, console_events, console_clear, engine_version, open_demo, scene, profile_runs, profile_band, add_profile_sketch, undo, redo, edit_state } from '../wasm/cave_wasm.js';

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
  omitted: number;
  note: string | null;
  /** [minS, minZ, maxS, maxZ] */
  bounds: [number, number, number, number] | null;
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
  profileRuns: (adjusted: boolean): RunInfo[] => JSON.parse(profile_runs(adjusted)),
  profileBand: (run: string, adjusted: boolean): ProfileScene | null => JSON.parse(profile_band(run, adjusted)),
  /** Adds a stroke drawn on a run's band; every edit goes through the document's one edit path. */
  addSketch: (run: string, points: [number, number][], widths: number[] | null): string => add_profile_sketch(run, JSON.stringify(points), widths ? JSON.stringify(widths) : ''),
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
