// Loads the Rust engine (WASM) once and re-exports the few calls the UI uses.
import init, { start, log, console_events, console_clear, engine_version, open_demo, scene } from '../wasm/cave_wasm.js';

export type Level = 'debug' | 'detail' | 'info' | 'warn' | 'error';
export interface ConsoleEvent {
  seq: number;
  level: Level;
  source: string;
  message: string;
}
export type Pt = [number, number];
export interface Scene {
  stations: { label: string; at: Pt; z: number; anchor: boolean }[];
  legs: { from: Pt; to: Pt; style: 'new' | 'closure' | 'tie' }[];
  splays: [number, number, number, number][];
  closures: { at: Pt; horizontal: number; vertical: number; percent: number | null }[];
  bounds: [number, number, number, number] | null;
  adjusted: boolean;
  skipped: number;
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
