// Loads the Rust engine (WASM) once and re-exports the few calls the UI uses.
import init, { start, log, console_events, console_clear, engine_version, open_demo, scene, profile_runs, profile_band, add_profile_sketch, undo, redo, edit_state, passage_mesh, shading, palette, open_dxf, save_dxf, section_stations, section_at, add_section_sketch, map_sections, place_section, map_profile_sketches, add_map_line, map_lines, draw_tools, edit_line, delete_line, set_line_closed, set_line_kind, line_info, set_line_flip, set_line_text, ornament, callout, flipped_lines, revision, station_rows, set_station_mark, symbols, fills, fill_marks, draw_tool_groups, sync_outbox, sync_apply, sync_snapshot, sync_load } from '../wasm/cave_wasm.js';

import { setInkPalette } from './ink';
import { setOrnamentSource } from './section-glyph';
import { setCalloutSource, setSymbolCatalogue, type SymbolDef } from './symbols';
import { setFillSource, type FillDef, type Marks } from './fills';
import type { StationRow } from './station-table';

/** The panels that keep their own undo history. */
export type Scope = 'profile' | 'section' | 'map' | 'stations';

export type Level = 'debug' | 'detail' | 'info' | 'warn' | 'error';
export interface ConsoleEvent {
  seq: number;
  level: Level;
  source: string;
  message: string;
}
export type Pt = [number, number, number];
export interface Scene {
  stations: { id: string; label: string; at: Pt; anchor: boolean }[];
  /** Palette types with something in this cave, and its trips: what the filter can list. */
  types: string[];
  trips: { id: string; name: string }[];
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
  /** Near-level splays as [s, z] ticks. */
  flat: [number, number][];
  sketches: { id: string; kind: string; points: [number, number][]; widths: number[] | null; closed: boolean; text?: string }[];
  /** Cross-section sketches of the stations on this band. */
  sections: BandSection[];
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
/** How one type looks (colour role, weight, linetype); the one table the viewer and the DXF export share. */
export interface TypeStyle {
  key: string;
  label: string;
  ink: 'text' | 'muted' | 'accent' | 'warn' | 'danger' | 'station' | 'geology';
  weight: number;
  linetype: 'solid' | 'dashed' | 'longdash' | 'sparse' | 'dashdot';
  opacity: number;
  under_opacity: number;
  layer: string;
}
/** A station's cross-section sketches as the map shows them, next to the station. */
export interface MapSection {
  id: string;
  label: string;
  /** The station in the map's metres. */
  at: Pt;
  sketches: { id: string; kind: string; points: [number, number][]; widths: number[] | null; closed: boolean; text?: string }[];
  /** Offset from the station placed by hand (east, north, up), or null until placed. */
  placement: Pt | null;
}
/** A profile sketch placed along its run in the cave: points are [east, north, up] metres. */
export interface ProfileSketch3d {
  /** Words that go with it (a symbol's label, or the text of a text stroke). */
  text?: string;
  id: string;
  closed: boolean;
  kind: string;
  run: string;
  points: [number, number, number][];
  widths: number[] | null;
}
/** The same on a profile band. */
export interface BandSection {
  id: string;
  label: string;
  s: number;
  z: number;
  sketches: { id: string; kind: string; points: [number, number][]; widths: number[] | null; closed: boolean; text?: string }[];
  /** Offset (along the band, elevation) placed by hand, or null until placed. */
  placement: [number, number] | null;
}
/** A station a cross section can be cut at. */
export interface SectionStation {
  id: string;
  label: string;
  /** Enough wall measurements for an outline. */
  outlined: boolean;
  /** Sketches already on its section. */
  sketches: number;
}
/** A stroke sketched on a cross section: [across, up] metres from the station. */
export interface SectionSketch {
  id: string;
  closed: boolean;
  /** What it is (`wall`, `ledge`, ...): the palette type that styles it. */
  kind: string;
  points: [number, number][];
  widths: number[] | null;
  text?: string;
}
/** The cross section at one station. Points are [across, up] in metres from the station. */
export interface StationSection {
  id: string;
  label: string;
  outline: [number, number][];
  measured: [number, number][];
  inferred: boolean;
  /** What "up the page" means: 'up', or 'north' in a shaft. */
  up_means: 'up' | 'north';
  /** Why there is no outline. */
  note: string | null;
  sketches: SectionSketch[];
}
export interface BuildInfo {
  builtAt: string | null;
  git: string;
  dirty: boolean;
  error: string | null;
}

let ready: Promise<void> | null = null;

export function loadEngine(): Promise<void> {
  ready ??= init().then(() => {
    start();
    setInkPalette(JSON.parse(palette()));
    setSymbolCatalogue(JSON.parse(symbols()));
    setCalloutSource((tip, at, chars) => JSON.parse(callout(tip[0], tip[1], at[0], at[1], chars, false)));
    setFillSource(engine);
  });
  return ready;
}

export const engine = {
  version: () => engine_version(),
  log: (level: Level, source: string, message: string) => log(level, source, message),
  events: (since: number): ConsoleEvent[] => JSON.parse(console_events(BigInt(since))),
  clear: () => console_clear(),
  openDemo: (json: string): string => open_demo(json),
  /** Every kind of area fill, with what it means. */
  fills: (): FillDef[] => JSON.parse(fills()),
  /** The marks that fill an outline (metres), seeded by the stroke's id. */
  fillMarks: (kind: string, curve: [number, number][], id: string): Marks => JSON.parse(fill_marks(kind, JSON.stringify(curve), id)),
  /** Every point symbol the palette offers, with its drawing. */
  symbols: (): SymbolDef[] => JSON.parse(symbols()),
  /** One row per station: what kind of place it is, its notes, elevation and the team's mark. */
  stationRows: (adjusted: boolean, keyword = ''): StationRow[] => JSON.parse(station_rows(adjusted, keyword)),
  /** Puts the team's mark on a station (one undoable edit in the Stations panel's history). */
  setStationMark: (id: string, status: string, team: string, who: string): string => set_station_mark(id, status, team, who),
  /** Goes up whenever the open cave really changes; compared with the value at the last save. */
  revision: (): number => revision(),
  /** Local edits since the last call (JSON ops, as a string; `[]` when none). */
  syncOutbox: (): string => sync_outbox(),
  /** Applies ops made on another device; returns how many applied. */
  syncApply: (ops: string): number => sync_apply(ops),
  /** The whole cave as JSON ops. */
  syncSnapshot: (): string => sync_snapshot(),
  /** Replaces the open cave with one rebuilt from ops; `ok` or `error: ...`. */
  syncLoad: (ops: string): string => sync_load(ops),
  scene: (adjusted: boolean, filter = '{}'): Scene => JSON.parse(scene(adjusted, filter)),
  /** Which drawn lines run under another passage, for a camera looking from `dir` toward the cave (for perspective, `dir` is the eye's position). */
  shading: (adjusted: boolean, dir: [number, number, number], perspective: boolean, filter = '{}'): Scene['shading'] => JSON.parse(shading(adjusted, dir[0], dir[1], dir[2], perspective, filter)),
  /** Opens a cave from DXF text; nothing changes unless it opens completely. */
  openDxf: (text: string): { ok: boolean; message: string; warnings: string[] } => JSON.parse(open_dxf(text)),
  /** The open cave as DXF text (the whole cave is inside the file). */
  saveDxf: (adjusted: boolean): string => save_dxf(adjusted),
  /** The stations a cross section can be cut at. */
  sectionStations: (adjusted: boolean, fillGaps: boolean, filter = '{}'): SectionStation[] => JSON.parse(section_stations(adjusted, fillGaps, filter)),
  /** The cross section at a station, with its sketches. */
  sectionAt: (adjusted: boolean, station: string, fillGaps: boolean, filter = '{}'): StationSection | null => JSON.parse(section_at(adjusted, station, fillGaps, filter)),
  /** Adds a stroke drawn on a station's cross section; every edit goes through the document's one edit path. */
  addSectionSketch: (station: string, kind: string, points: [number, number][], widths: number[] | null, closed: boolean, adjusted: boolean, text = ''): string => add_section_sketch(station, kind, JSON.stringify(points), widths ? JSON.stringify(widths) : '', closed, adjusted, text),
  /** The cross-section sketches to show on the map. */
  mapSections: (adjusted: boolean, filter = '{}'): MapSection[] => JSON.parse(map_sections(adjusted, filter)),
  /** The profile sketches placed along their runs in 3D, for the map. */
  mapProfileSketches: (adjusted: boolean): ProfileSketch3d[] => JSON.parse(map_profile_sketches(adjusted)),
  /** Places a station's section sketches by hand in a view (an undoable edit). */
  placeSection: (station: string, view: 'map' | 'profile', offset: [number, number, number]): string => place_section(station, view, offset[0], offset[1], offset[2]),
  /** Adds a line of `kind` drawn in plan on the map (anchored to the survey, undoable). */
  addMapLine: (kind: string, points: [number, number][], widths: number[] | null, closed: boolean, adjusted: boolean, text = ''): string => add_map_line(kind, JSON.stringify(points), widths ? JSON.stringify(widths) : '', closed, adjusted, text),
  /** Moves the fit points of a line already drawn (in the space it was drawn in); re-anchors it to the survey. */
  editLine: (id: string, points: [number, number][], widths: number[] | null, adjusted: boolean): string => edit_line(id, JSON.stringify(points), widths ? JSON.stringify(widths) : '', adjusted),
  /** Closes or opens a line already drawn. */
  setLineClosed: (id: string, closed: boolean): string => set_line_closed(id, closed),
  /** Changes what a line already drawn is (wall, ledge, ...). */
  setLineKind: (id: string, kind: string): string => set_line_kind(id, kind),
  /** What a drawn line is, or null when the id is not a drawn line. */
  setLineText: (id: string, text: string): string => set_line_text(id, text),
  setLineFlip: (id: string, flip: boolean): string => set_line_flip(id, flip),
  ornament: (kind: string, curve: [number, number][], closed: boolean, flip: boolean, id: string, survey: [number, number][]): { spine: boolean; lines: [number, number][][] } => JSON.parse(ornament(kind, JSON.stringify(curve), closed, flip, id, JSON.stringify(survey))),
  flippedLines: (): string[] => JSON.parse(flipped_lines()),
  lineInfo: (id: string): { kind: string; view: 'profile' | 'section' | 'map'; closed: boolean; flip: boolean; text: string } | null => JSON.parse(line_info(id)),
  /** Deletes a line drawn in a view. */
  deleteLine: (id: string): string => delete_line(id),
  /** The linework drawn in plan on the map, lifted to the passage's elevation. */
  mapLines: (adjusted: boolean): ProfileSketch3d[] => JSON.parse(map_lines(adjusted)),
  /** The drawing tools the Draw tab offers. */
  drawTools: (): TypeStyle[] => JSON.parse(draw_tools()),
  /** The drawing tools as the Draw tab groups them. */
  drawToolGroups: (): { name: string; tools: TypeStyle[] }[] => JSON.parse(draw_tool_groups()),
  /** The type palette. */
  palette: (): TypeStyle[] => JSON.parse(palette()),
  profileRuns: (adjusted: boolean): RunInfo[] => JSON.parse(profile_runs(adjusted)),
  profileBand: (run: string, adjusted: boolean): ProfileScene | null => JSON.parse(profile_band(run, adjusted)),
  /** Adds a stroke drawn on a run's band; every edit goes through the document's one edit path. */
  addSketch: (run: string, kind: string, points: [number, number][], widths: number[] | null, closed: boolean, adjusted: boolean, text = ''): string => add_profile_sketch(run, kind, JSON.stringify(points), widths ? JSON.stringify(widths) : '', closed, adjusted, text),
  passageMesh: (adjusted: boolean, mode: 'trip' | 'depth', fillGaps: boolean, filter = '{}'): PassageBuffers => {
    const m = passage_mesh(adjusted, mode, fillGaps, filter);
    const out = { positions: m.positions, normals: m.normals, colors: m.colors, filled: m.filled };
    m.free();
    return out;
  },
  /** Undo and redo belong to a panel: each only touches what was done in that panel. */
  undo: (scope: Scope): boolean => undo(scope),
  redo: (scope: Scope): boolean => redo(scope),
  editState: (scope: Scope): { undo: boolean; redo: boolean } => JSON.parse(edit_state(scope)),
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

setOrnamentSource(engine); // shaped-line marks are drawn by the 2D canvases from the engine

// Dev-only handle for tests and debugging in the browser console.
if (import.meta.env.DEV) (window as unknown as { __engine: typeof engine }).__engine = engine;
