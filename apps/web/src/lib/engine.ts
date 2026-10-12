// Loads the Rust engine (WASM) once and re-exports the few calls the UI uses.
import init, { start, log, console_events, console_clear, engine_version, open_demo, scene, profile_runs, profile_band, add_profile_sketch, undo, redo, edit_state, passage_mesh, shading, palette, open_dxf, save_dxf, section_stations, section_at, add_section_sketch, map_sections, place_section, map_profile_sketches, add_map_line, map_lines, draw_tools, edit_line, delete_line, set_line_closed, set_line_kind, line_info, set_line_flip, set_line_text, ornament, callout, sheet_options, sheet_plan, flipped_lines, revision, station_rows, set_station_mark, station_detail, shot_detail, set_shot, set_station_walls, set_station_note, set_station_benchmark, symbols, fills, fill_marks, draw_tool_groups, sync_outbox, sync_apply, sync_snapshot, sync_load, notebook_trips, notebook_page, notebook_set_reading, notebook_set_walls, notebook_set_note, notebook_set_trip_field, notebook_set_declination, notebook_add_shot, notebook_remove_shot, notebook_rename_station, notebook_new_trip, scan_links, scan_link, scan_unlink, scan_placements, scan_alignment, scan_add_anchor, scan_remove_anchor, scan_set_mode, scan_trim, scan_set_trim, scan_view_stations, new_cave } from '../wasm/cave_wasm.js';

import { setInkPalette } from './ink';
import { setOrnamentSource } from './section-glyph';
import { setCalloutSource, setSymbolCatalogue, type SymbolDef } from './symbols';
import { setFillSource, type FillDef, type Marks } from './fills';
import type { StationRow } from './station-table';
import type { Lrud, NotebookPage, TripChoice } from './notebook';

/** The panels that keep their own undo history. */
export type Scope = 'profile' | 'section' | 'map' | 'stations' | 'notebook' | 'scans';

export type Level = 'debug' | 'detail' | 'info' | 'warn' | 'error';
export interface ConsoleEvent {
  seq: number;
  level: Level;
  source: string;
  message: string;
}
export type Pt = [number, number, number];
export interface Scene {
  stations: { id: string; label: string; at: Pt; anchor: boolean; benchmark: boolean }[];
  /** Palette types with something in this cave, and its trips: what the filter can list. */
  types: string[];
  trips: { id: string; name: string }[];
  legs: { shot: string; from: Pt; to: Pt; style: 'new' | 'closure' | 'tie' }[];
  /** [x1, y1, z1, x2, y2, z2] */
  splays: [number, number, number, number, number, number][];
  /** The shot (id) of each splay, in the same order. */
  splay_shots: string[];
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
/** Passage dimensions at a station, metres (null: not measured). */
export interface StationWalls {
  left: number | null;
  right: number | null;
  up: number | null;
  down: number | null;
  left_open: boolean;
  right_open: boolean;
  up_open: boolean;
  down_open: boolean;
}
/** What a person can change about a shot. */
export interface ShotEdit {
  distance: number | null;
  azimuth: number | null;
  inclination: number | null;
  back_azimuth: number | null;
  back_inclination: number | null;
  exclude_from_plot: boolean;
  exclude_from_all: boolean;
  no_adjust: boolean;
}
/** What the shot tab shows (a leg or a splay). */
export interface ShotDetail extends ShotEdit {
  id: string;
  splay: boolean;
  from: string;
  to: string;
  trip: number;
  trip_name: string;
  note: string;
  problem: string;
}
/** What the station tab shows. */
export interface StationDetail {
  id: string;
  label: string;
  kinds: string[];
  degree: number;
  trips: number[];
  z: number | null;
  flags: string[];
  benchmark: boolean;
  walls: (StationWalls & { on: string }) | null;
  notes: { source: string; on: string; text: string }[];
}
/** Papers (inches) and plot scales a sheet offers. */
export interface SheetOptions {
  papers: { name: string; w: number; h: number }[];
  scales: { label: string; feet_per_inch: number; metric: boolean }[];
}
/** What a person chose for a sheet. */
export interface SheetSetup {
  /** Index into the papers. */
  paper: number;
  /** 0 landscape, 1 portrait, 2 whichever fits the cave better. */
  turned: 0 | 1 | 2;
  /** Feet of cave per inch of paper; 0 for the most detailed scale that fits. */
  feetPerInch: number;
  name: string;
  lines: string[];
  title: boolean;
  bar: boolean;
  north: boolean;
}
/** A shape of the sheet's furniture, in inches from the paper's lower left corner. */
export type SheetShape = { t: 'line'; a: [number, number]; b: [number, number] } | { t: 'rect'; a: [number, number]; b: [number, number] } | { t: 'fill'; pts: [number, number][] } | { t: 'text'; at: [number, number]; h: number; s: string };
/** A sheet laid out: paper size (inches), the box the map may use, the scale and the furniture. */
export interface SheetPlan {
  w: number;
  h: number;
  map: [number, number, number, number];
  feet_per_inch: number;
  shapes: SheetShape[];
  turned: boolean;
  fits: boolean;
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
/** A scan placed under the map: `matrix` takes a pixel (from the top left, y down) to the map in metres. */
export interface ScanPlacement {
  trip: string;
  path: string;
  matrix: [number, number, number, number, number, number];
  used: number;
  rms: number;
  mode: 'rigid' | 'warp';
  /** The outline it is trimmed to (scan pixels), or null for the whole scan. */
  trim: [number, number][] | null;
}
/** The spots on a scan matched to stations. */
export interface ScanAlignment {
  mode: 'rigid' | 'warp';
  anchors: { px: number; py: number; station: string; label: string; x: number | null; y: number | null }[];
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
  /** Everything the station tab shows about one station, or null. */
  stationDetail: (id: string, adjusted: boolean): StationDetail | null => JSON.parse(station_detail(id, adjusted)),
  /** Everything the shot tab shows about one leg or splay, or null. */
  shotDetail: (id: string): ShotDetail | null => JSON.parse(shot_detail(id)),
  /** Changes what is measured on a shot (metres, degrees); only what changed is written. */
  setShot: (id: string, e: ShotEdit): string => set_shot(id, JSON.stringify(e)),
  /** Writes a station's passage dimensions (metres) to the survey; only what changed. */
  setStationWalls: (id: string, w: StationWalls): string => set_station_walls(id, JSON.stringify(w)),
  /** Changes a note written at a station (`source` comes from the detail). */
  setStationNote: (source: string, text: string): string => set_station_note(source, text),
  /** Marks a station as an agreed benchmark, or not. */
  setStationBenchmark: (id: string, on: boolean): string => set_station_benchmark(id, on),
  setStationMark: (id: string, status: string, team: string, who: string): string => set_station_mark(id, status, team, who),
  /** The trips, for the notebook's page chooser. */
  notebookTrips: (): TripChoice[] => JSON.parse(notebook_trips()),
  /** One trip laid out as a notes page, or null. */
  notebookPage: (trip: string): NotebookPage | null => JSON.parse(notebook_page(trip)),
  /** Writes one reading of a shot (tape in metres; compass cells are magnetic degrees); blank clears it. */
  notebookSetReading: (shot: string, which: 'distance' | 'az_fs' | 'az_bs' | 'inc_fs' | 'inc_bs', value: number | null): string => notebook_set_reading(shot, which, value === null ? '' : String(value)),
  /** Writes the dimensions beside a station line (`target` is the arriving shot, or the station). */
  notebookSetWalls: (target: string, w: Lrud): string => notebook_set_walls(target, JSON.stringify(w)),
  /** Writes the note on a notebook line (`source` from the page). */
  notebookSetNote: (source: string, text: string): string => notebook_set_note(source, text),
  notebookSetTripField: (trip: string, field: 'name' | 'date' | 'team' | 'instruments', text: string): string => notebook_set_trip_field(trip, field, text),
  /** Changes a trip's declination: every bearing in the trip turns by the difference. */
  notebookSetDeclination: (trip: string, degrees: number): string => notebook_set_declination(trip, degrees),
  /** Adds a shot at the end of a page (`to` empty for a splay). */
  notebookAddShot: (trip: string, from: string, to: string, splay: boolean): string => notebook_add_shot(trip, from, to, splay),
  notebookRemoveShot: (shot: string): string => notebook_remove_shot(shot),
  notebookRenameStation: (station: string, name: string): string => notebook_rename_station(station, name),
  /** Starts a new trip; returns its id, or `error: ...`. */
  notebookNewTrip: (name: string): string => notebook_new_trip(name),
  /** Every scan linked to a trip (paths relative to the scans folder). */
  scanLinks: (): { trip: string; path: string }[] => JSON.parse(scan_links()),
  /** Links a scan to a trip (one undoable edit in the Scans panel's history). */
  scanLink: (trip: string, path: string): string => scan_link(trip, path),
  scanUnlink: (trip: string, path: string): string => scan_unlink(trip, path),
  /** Where each scan placed in a view sits now (`map`, `profile:<run>` or `section:<station>`). */
  scanPlacements: (view: string, adjusted: boolean): ScanPlacement[] => JSON.parse(scan_placements(view, adjusted)),
  /** The stations a view shows: what a spot on a scan can be matched to there. */
  scanViewStations: (view: string, adjusted: boolean): { id: string; label: string }[] => JSON.parse(scan_view_stations(view, adjusted)),
  scanAlignment: (trip: string, view: string, path: string): ScanAlignment | null => JSON.parse(scan_alignment(trip, view, path)),
  /** Matches a spot on a scan (pixels from its top left) with a station (its id) or a point in the view (`x:y`, metres). */
  scanAddAnchor: (trip: string, view: string, path: string, px: number, py: number, target: string): string => scan_add_anchor(trip, view, path, px, py, target),
  scanRemoveAnchor: (trip: string, view: string, path: string, index: number): string => scan_remove_anchor(trip, view, path, index),
  scanSetMode: (trip: string, view: string, path: string, mode: 'rigid' | 'warp'): string => scan_set_mode(trip, view, path, mode),
  /** The outline a scan is trimmed to in a view (scan pixels), or null. */
  scanTrim: (trip: string, view: string, path: string): [number, number][] | null => JSON.parse(scan_trim(trip, view, path)),
  /** Trims a scan to a traced outline (thinned to within `tolerance` pixels); an empty outline takes the trim off. */
  scanSetTrim: (trip: string, view: string, path: string, outline: [number, number][], tolerance = 1.5): string => scan_set_trim(trip, view, path, JSON.stringify(outline), tolerance),
  /** Starts a brand new cave with one empty trip (`ft` or `m`); returns `ok` or `error: ...`. */
  newCave: (name: string, trip: string, unit: 'ft' | 'm'): string => new_cave(name, trip, unit),
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
  /** The papers and plot scales a sheet offers. */
  sheetOptions: (): SheetOptions => JSON.parse(sheet_options()),
  /** A sheet laid out for a cave this many metres wide and tall (see `sheet_plan` in the engine). */
  sheetPlan: (width: number, height: number, p: SheetSetup): SheetPlan => JSON.parse(sheet_plan(width, height, p.paper, p.turned, p.feetPerInch, p.name, JSON.stringify(p.lines), p.title, p.bar, p.north)),
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
