// Loads the Rust engine (WASM) once and re-exports the few calls the UI uses.
import type { FillDef as PatternFillDef, LineDef, Motif, PatternKind, PatternLibrary, PatternMarks, Prim } from './patterns';
import init, { start, log, console_events, console_clear, engine_version, open_demo, scene, profile_runs, profile_band, add_profile_sketch, undo, redo, edit_state, passage_mesh, shading, palette, open_dxf, save_dxf, section_stations, section_at, add_section_sketch, map_sections, place_section, map_profile_sketches, add_map_line, paste_lines, last_added_id, map_lines, draw_tools, edit_line, edit_leader, delete_line, set_line_closed, set_line_kind, line_info, set_line_flip, set_line_style, drawing_scale, set_drawing_scale, room_id, set_room_id, linetypes, pattern_library, motif_from_drawing, save_pattern, remove_pattern, check_pattern, line_pattern_marks, fill_pattern_marks, line_pattern_marks_named, fill_pattern_marks_named, set_line_text, ornament, callout, sheet_options, sheet_plan, flipped_lines, revision, station_rows, set_station_mark, station_detail, shot_detail, set_shot, set_station_walls, set_station_note, set_station_benchmark, symbols, fills, fill_marks, draw_tool_groups, sync_outbox, sync_apply, sync_snapshot, sync_load, notebook_trips, notebook_page, notebook_set_reading, notebook_set_walls, notebook_set_note, notebook_set_trip_field, notebook_set_declination, notebook_add_shot, notebook_remove_shot, notebook_rename_station, notebook_new_trip, scan_links, scan_link, scan_unlink, scan_placements, scan_alignment, scan_add_anchor, scan_remove_anchor, scan_set_mode, scan_trim, scan_set_trim, scan_view_stations, new_cave, scan_completed, scan_set_complete, shelf_summary, shelf_summarize, shelf_summarize_demo, shelf_summarize_native, room_id_of_native, survey_sync_trip, survey_snapshot, survey_connect, survey_frame, survey_flush_link, survey_disconnect, survey_link_down, survey_sim_station, survey_settle_now, survey_accept, survey_discard, survey_undo_last, survey_set_mode, survey_set_backsights, survey_set_auto, survey_skip_backsight, survey_next_station, survey_set_names, survey_set_standing_wall, instrument_catalog, instrument_identify, sketch_source, sketch_views, sketch_anchor, sketch_place, snap_pick, snap_commit, snap_session, snap_along, snap_catalog, survey_formats, import_survey, import_folder, export_survey, geo_status, geo_set_location, surface_plan, surface_plan_for_record, surface_ground, surface_commit, set_contour_interval, surface_layers, surface_terrain, surface_pick } from '../wasm/cave_wasm.js';

import { setInkPalette } from './ink';
import { setOrnamentSource } from './section-glyph';
import { setPatternSource } from './pattern-draw';
import { setCalloutSource, setSymbolCatalogue, type SymbolDef } from './symbols';
import type { Warp } from './scan-warp';
import { setFillSource, type FillDef, type Marks } from './fills';
import { guardEngine } from './readonly';
import type { StationRow } from './station-table';
import type { Lrud, NotebookPage, TripChoice } from './notebook';
import type { Catalog, Identified, Step } from './survey-types';
import type { Anchored, Mark, SketchSource, SketchViews } from './sketch-types';
import type { SnapAcquired, SnapCatalog, SnapGuide, SnapMode, SnapOutcome, SnapResult, SnapScene, SnapSession, SnapSettings, V3 } from './snap-types';

// Surface Data and the entrance location (cave-wasm/src/geo.rs). Coordinates stay on this screen: never log them.
export interface GeoLocation { station: string; lat: number; lon: number; ground_m?: number; source: string }
export interface SurfaceRecord { zone: number; bounds: { xmin: number; ymin: number; xmax: number; ymax: number }; photo?: [number, number]; elevation?: [number, number]; for_at: { lat: number; lon: number }; fetched: string }
export interface GeoStatus {
  location: GeoLocation | null;
  surface: SurfaceRecord | null;
  candidate: { kind: 'stored' } | { kind: 'imported'; station: string; lat: number; lon: number } | { kind: 'ask'; station: string | null };
  interval_m: number | null;
  stations: { id: string; label: string }[];
}
export interface SurfacePlan { zone: number; bounds: SurfaceRecord['bounds']; for_at: { lat: number; lon: number }; photo?: { w: number; h: number; url: string }; elevation?: { w: number; h: number; url: string } }
export interface SurfaceContour { level: number; major: boolean; closed: boolean; xyz: number[] }
export interface SurfaceLayers { photo_matrix: number[] | null; photo_size: [number, number] | null; contours: SurfaceContour[]; stale: boolean }

/** The panels that keep their own undo history (`settings`: the cave's own settings, such as its location and surface). */
export type Scope = 'profile' | 'section' | 'map' | 'stations' | 'notebook' | 'scans' | 'settings';

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
  sketches: { id: string; kind: string; points: [number, number][]; widths: number[] | null; closed: boolean; text?: string; style?: string }[];
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
  /** The same pattern on paper in millimetres (a dash positive, a gap negative, a dot zero). */
  pattern: number[];
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
  sketches: { id: string; kind: string; points: [number, number][]; widths: number[] | null; closed: boolean; text?: string; style?: string }[];
  /** Offset from the station placed by hand (east, north, up), or null until placed. */
  placement: Pt | null;
}
/** A profile sketch placed along its run in the cave: points are [east, north, up] metres. */
export interface ProfileSketch3d {
  /** Words that go with it (a symbol's label, or the text of a text stroke). */
  text?: string;
  style?: string;
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
  sketches: { id: string; kind: string; points: [number, number][]; widths: number[] | null; closed: boolean; text?: string; style?: string }[];
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
  style?: string;
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
  mode: 'rigid' | 'warp' | 'rubber';
  /** The rubber-sheet warp when the scan is fitted that way (it replaces the matrix for drawing). */
  warp?: Warp | null;
  /** The stations its matches are (ids), so a view can lay the scan at their height. */
  stations?: string[];
  /** The outline it is trimmed to (scan pixels), or null for the whole scan. */
  trim: [number, number][] | null;
}
/** The spots on a scan matched to stations. */
export interface ScanAlignment {
  mode: 'rigid' | 'warp' | 'rubber';
  anchors: { px: number; py: number; station: string; label: string; x: number | null; y: number | null }[];
}
/** One trip's line in the Cave Shelf. */
export interface ShelfTrip {
  id: string;
  index: number;
  name: string;
  date: string;
  team: string;
  declination: number | null;
  unit: 'ft' | 'm';
  shots: number;
  length_m: number;
  start: string;
  end: string;
  ends: string[];
}
/** What the Cave Shelf shows about a cave. */
export interface CaveSummary {
  name: string;
  unit: 'ft' | 'm';
  length_m: number;
  depth_m: number | null;
  stations: number;
  shots: number;
  trips: ShelfTrip[];
  ends: { station: string; trip: number }[];
  loops: number;
  worst_percent: number | null;
  empty: boolean;
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
    setCalloutSource((tips, at, chars, shape, height) => JSON.parse(callout(JSON.stringify(tips), at[0], at[1], chars, shape, height)));
    setFillSource(engine);
  });
  return ready;
}

/** A survey data format (see `cave-formats`). */
export interface SurveyFormat {
  id: string;
  label: string;
  extensions: string[];
}

const rawEngine = {
  version: () => engine_version(),
  log: (level: Level, source: string, message: string) => log(level, source, message),
  events: (since: number): ConsoleEvent[] => JSON.parse(console_events(BigInt(since))),
  clear: () => console_clear(),
  openDemo: (json: string): string => open_demo(json),
  /** Every kind of area fill, with what it means. */
  fills: (): FillDef[] => JSON.parse(fills()),
  /** The marks that fill an outline (metres), seeded by the stroke's id. */
  fillMarks: (kind: string, curve: [number, number][], id: string, amount = 1, length = 1, angle = 0): Marks => JSON.parse(fill_marks(kind, JSON.stringify(curve), id, amount, length, angle)),
  /** The drawing's scale, the N of 1:N. */
  drawingScale: (): number => drawing_scale(),
  setDrawingScale: (n: number): string => set_drawing_scale(n),
  /** The cave's location, downloaded surface and candidate entrance (`{ error }` with no cave open). */
  geoStatus: (): GeoStatus | { error: string } => { const t = geo_status(); return t.startsWith('error') ? { error: t.slice(7) } : JSON.parse(t); },
  /** Stores the cave's location (one undoable edit on `scope`'s history); `ok` or `error: ...`. */
  geoSetLocation: (l: GeoLocation, scope: Scope = 'settings'): string => geo_set_location(JSON.stringify(l), scope),
  /** What to download for the open cave. */
  surfacePlan: (photo: boolean, elevation: boolean, adjusted: boolean): SurfacePlan | { error: string } => { const t = surface_plan(photo, elevation, adjusted); return t.startsWith('error') ? { error: t.slice(7) } : JSON.parse(t); },
  /** The plan that downloads again exactly what the stored surface record lists (no new window). */
  surfacePlanForRecord: (adjusted: boolean): SurfacePlan | { error: string } => { const t = surface_plan_for_record(adjusted); return t.startsWith('error') ? { error: t.slice(7) } : JSON.parse(t); },
  /** Ground (m NAVD88) at the stored location from an elevation TIFF, or NaN. */
  surfaceGround: (plan: SurfacePlan, tiff: Uint8Array): number => surface_ground(JSON.stringify(plan), tiff),
  /** Records a finished download (one edit on `scope`'s history); a NaN ground leaves the stored ground alone. */
  surfaceCommit: (plan: SurfacePlan, groundM: number, today: string, scope: Scope = 'settings'): string => surface_commit(JSON.stringify(plan), groundM, today, scope),
  /** Sets the contour spacing in metres (one edit). */
  setContourInterval: (m: number): string => set_contour_interval(m),
  /** The photo matrix and contours in the cave frame (`{ error }` when there is no surface). */
  surfaceLayers: (tiff: Uint8Array, interval: number, feet: boolean, adjusted: boolean): SurfaceLayers | { error: string } => { const t = surface_layers(tiff, interval, feet, adjusted); return t.startsWith('error') ? { error: t.slice(7) } : JSON.parse(t); },
  /** The ground mesh in the cave frame as numbers (xyz is x,y,z per vertex row by row, NaN z = hole); null when there is no surface or datum. */
  surfaceTerrain: (tiff: Uint8Array, adjusted: boolean): { w: number; h: number; xyz: Float32Array } | null => { const a = surface_terrain(tiff, adjusted); return a.length < 2 ? null : { w: a[0], h: a[1], xyz: a.subarray(2) }; },
  /** The entrance pick: lowest ground inside a circle (cave metres). */
  surfacePick: (tiff: Uint8Array, cx: number, cy: number, r: number, adjusted: boolean): { x: number; y: number; lat: number; lon: number; ground_m: number } | { error: string } => { const t = surface_pick(tiff, cx, cy, r, adjusted); return t.startsWith('error') ? { error: t.slice(7) } : JSON.parse(t); },
  /** The open cave's room id on the server ('' when it has none). */
  roomId: (): string => room_id(),
  /** Sets the open cave's room id; `ok` or `error: ...`. */
  setRoomId: (id: string): string => set_room_id(id),
  /** The line patterns a line can be given, on paper in millimetres. */
  linetypes: (): { dot_mm: number; types: { key: string; pattern: number[] }[] } => JSON.parse(linetypes()),
  /** The motifs, linetypes and fills the cave can pick from: the built-in ones and its own (`own`). */
  patternLibrary: (): PatternLibrary => JSON.parse(pattern_library()),
  /** A drawing (any size and place) as a motif called `name`; a string starting `error:` says why not. Nothing is saved. */
  motifFromDrawing: (name: string, prims: Prim[]): Motif | string => {
    const r = motif_from_drawing(name, JSON.stringify(prims));
    return r.startsWith('error:') ? r : JSON.parse(r);
  },
  /** Saves a pattern of `kind` as one undoable edit: `ok` or `error: ...`. */
  savePattern: (kind: PatternKind, def: Motif | LineDef | PatternFillDef): string => save_pattern(kind, JSON.stringify(def)),
  /** Removes one of the cave's own patterns: `ok` or `error: ...`. */
  removePattern: (kind: PatternKind, name: string): string => remove_pattern(kind, name),
  /** Checks a definition without saving it: `ok` or `error: ...` in plain words. */
  checkPattern: (kind: 'line' | 'fill', def: LineDef | PatternFillDef): string => check_pattern(kind, JSON.stringify(def)),
  /** The marks of a linetype laid along a path (metres) at a drawing scale. */
  linePatternMarks: (def: LineDef, path: [number, number][], scale: number): PatternMarks => JSON.parse(line_pattern_marks(JSON.stringify(def), JSON.stringify(path), scale)),
  /** The marks of the library's linetype called `name` along a path at a drawing scale (empty for an unknown name). */
  linePatternMarksNamed: (name: string, path: [number, number][], scale: number): PatternMarks => JSON.parse(line_pattern_marks_named(name, JSON.stringify(path), scale)),
  /** The marks of the library's fill called `name` inside an outline, seeded by `id`, at a drawing scale (empty for an unknown name). */
  fillPatternMarksNamed: (name: string, outline: [number, number][], id: string, amount: number, scale: number, angle = 0): PatternMarks => JSON.parse(fill_pattern_marks_named(name, JSON.stringify(outline), id, amount, scale, angle)),
  /** The marks of a fill inside an outline (metres), seeded by `id`, at a drawing scale. */
  fillPatternMarks: (def: PatternFillDef, outline: [number, number][], id: string, amount: number, scale: number): PatternMarks => JSON.parse(fill_pattern_marks(JSON.stringify(def), JSON.stringify(outline), id, amount, scale)),
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
  // ---- the in-cave Survey (cave-session, cave-instrument): every call answers with one step ----
  /** Makes the Survey session ready for a trip. */
  surveySyncTrip: (trip: string): Step => JSON.parse(survey_sync_trip(trip)),
  surveySnapshot: (): Step => JSON.parse(survey_snapshot()),
  /** Brings a link up: `x1`, `sap6`, `sap5`, `bric`, `capture` or `sim` (with `{device, quirk}`). */
  surveyConnect: (kind: string, options: object = {}): Step => JSON.parse(survey_connect(kind, JSON.stringify(options))),
  /** A frame from the instrument (`frame`, or a BRIC's `primary`, `meta`, `errors`). */
  surveyFrame: (kind: string, bytes: Uint8Array): Step => JSON.parse(survey_frame(kind, bytes)),
  surveyFlushLink: (): Step => JSON.parse(survey_flush_link()),
  surveyDisconnect: (): Step => JSON.parse(survey_disconnect()),
  surveyLinkDown: (): Step => JSON.parse(survey_link_down()),
  /** The simulator shoots a whole station. */
  surveySimStation: (walls: boolean): Step => JSON.parse(survey_sim_station(walls)),
  surveySettleNow: (): Step => JSON.parse(survey_settle_now()),
  surveyAccept: (): Step => JSON.parse(survey_accept()),
  surveyDiscard: (): Step => JSON.parse(survey_discard()),
  surveyUndoLast: (): Step => JSON.parse(survey_undo_last()),
  surveySetMode: (mode: string): Step => JSON.parse(survey_set_mode(mode)),
  surveySetBacksights: (on: boolean): Step => JSON.parse(survey_set_backsights(on)),
  surveySetAuto: (auto: boolean): Step => JSON.parse(survey_set_auto(auto)),
  surveySkipBacksight: (): Step => JSON.parse(survey_skip_backsight()),
  surveyNextStation: (): Step => JSON.parse(survey_next_station()),
  surveySetNames: (from: string, to: string): Step => JSON.parse(survey_set_names(from, to)),
  surveySetStandingWall: (side: string, value: number | null): Step => JSON.parse(survey_set_standing_wall(side, value === null ? '' : String(value))),
  /** The instruments' Bluetooth ids and names, and the simulator's devices and quirks. */
  instrumentCatalog: (): Catalog => JSON.parse(instrument_catalog()),
  instrumentIdentify: (name: string, services: string[]): { identified: Identified | null; nordicUart: boolean } => JSON.parse(instrument_identify(name, JSON.stringify(services))),
  // ---- object snap (cave-snap): what a point pick can land on ----
  /** The modes the settings list and the override menu offer, and the default settings. */
  snapCatalog: (): SnapCatalog => JSON.parse(snap_catalog()),
  /** What the cursor (pixels) can snap to in a scene. */
  snapPick: (scene: SnapScene, x: number, y: number, settings: SnapSettings, session: SnapSession): SnapResult => JSON.parse(snap_pick(JSON.stringify(scene), x, y, JSON.stringify(settings), JSON.stringify(session))),
  /** A pick landed on a world point: the point to use (null when the pick only set something up) and the session after it. */
  snapCommit: (session: SnapSession, p: V3): SnapOutcome => JSON.parse(snap_commit(JSON.stringify(session), p[0], p[1], p[2])),
  snapAcquire: (session: SnapSession, a: SnapAcquired): SnapSession => JSON.parse(snap_session('acquire', JSON.stringify(session), JSON.stringify(a))),
  /** The next pick searches only this mode (or sets the reference up); `null` returns to the running modes. */
  snapOnePick: (session: SnapSession, mode: SnapMode | null): SnapSession => JSON.parse(snap_session('one_pick', JSON.stringify(session), JSON.stringify(mode))),
  snapCancel: (session: SnapSession): SnapSession => JSON.parse(snap_session('cancel', JSON.stringify(session), 'null')),
  snapLastPoint: (session: SnapSession, p: V3 | null): SnapSession => JSON.parse(snap_session('last', JSON.stringify(session), JSON.stringify(p))),
  /** The point `distance` along a straight guide. */
  snapAlong: (guide: SnapGuide, distance: number): V3 | null => JSON.parse(snap_along(JSON.stringify(guide), distance)),
  // ---- the sketchpad's geometry (cave-sketch) ----
  /** A trip's page as the shots and passage dimensions the sketchpad draws. */
  sketchSource: (trip: string): SketchSource | null => JSON.parse(sketch_source(trip)),
  sketchViews: (source: SketchSource, station: string | null): SketchViews | null => JSON.parse(sketch_views(JSON.stringify(source), station ?? '')),
  sketchAnchor: (stations: Mark[], pts: [number, number][], kind: string, widths: number[] | null): Anchored | null => JSON.parse(sketch_anchor(JSON.stringify(stations), JSON.stringify(pts), kind, JSON.stringify(widths))),
  sketchPlace: (stations: Mark[], stroke: Anchored): [number, number][] | null => JSON.parse(sketch_place(JSON.stringify(stations), JSON.stringify(stroke))),
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
  scanSetMode: (trip: string, view: string, path: string, mode: 'rigid' | 'warp' | 'rubber'): string => scan_set_mode(trip, view, path, mode),
  /** The outline a scan is trimmed to in a view (scan pixels), or null. */
  scanTrim: (trip: string, view: string, path: string): [number, number][] | null => JSON.parse(scan_trim(trip, view, path)),
  /** Trims a scan to a traced outline (thinned to within `tolerance` pixels); an empty outline takes the trim off. */
  scanSetTrim: (trip: string, view: string, path: string, outline: [number, number][], tolerance = 1.5): string => scan_set_trim(trip, view, path, JSON.stringify(outline), tolerance),
  /** Every scanned page marked complete (paths). */
  scanCompleted: (): string[] => JSON.parse(scan_completed()),
  /** Marks pages complete or incomplete (one undoable edit in the Scans history). */
  scanSetComplete: (paths: string[], complete: boolean): string => scan_set_complete(JSON.stringify(paths), complete),
  /** What the shelf shows about the open cave, or null when none is open. */
  shelfSummary: (): CaveSummary | null => JSON.parse(shelf_summary()),
  /** The same for a cave kept as DXF text, without opening it (null when it is not a cave). */
  shelfSummarize: (dxf: string): CaveSummary | null => JSON.parse(shelf_summarize(dxf)),
  /** The same for a sample cave given as shot-list JSON. */
  shelfSummarizeDemo: (json: string): CaveSummary | null => JSON.parse(shelf_summarize_demo(json)),
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
  /** The survey data formats that can be imported and exported (Compass, Walls, Survex, Therion, CSV). */
  surveyFormats: (): SurveyFormat[] => JSON.parse(survey_formats()),
  /** Opens a survey data file as the cave. `others` are the other files picked with it, which a Therion file's `input` lines are found among. */
  importSurvey: (name: string, text: string, others: Record<string, string> = {}): { ok: boolean; message: string; warnings: string[] } => JSON.parse(import_survey(name, text, JSON.stringify(others))),
  /** Opens a whole folder (path to text) as the cave: through its project file when it has one, else every survey file in it. */
  importFolder: (files: Record<string, string>): { ok: boolean; message: string; warnings: string[] } => JSON.parse(import_folder(JSON.stringify(files))),
  /** The open cave as a survey data file in the format with this id. */
  exportSurvey: (format: string): { ok: boolean; text: string; extension: string; message: string } => JSON.parse(export_survey(format)),
  /** The whole cave in CaveCAD's own format: every op, as JSON text. */
  saveNative: (): string => sync_snapshot(),
  /** Opens a cave from its own format (JSON ops). Returns `ok` or `error: ...`. */
  openNative: (ops: string): string => sync_load(ops),
  shelfSummarizeNative: (ops: string): CaveSummary | null => JSON.parse(shelf_summarize_native(ops)),
  /** The room id of a cave kept in its own format, without opening it ('' when none). */
  roomIdOfNative: (ops: string): string => room_id_of_native(ops),
  /** The stations a cross section can be cut at. */
  sectionStations: (adjusted: boolean, fillGaps: boolean, filter = '{}'): SectionStation[] => JSON.parse(section_stations(adjusted, fillGaps, filter)),
  /** The cross section at a station, with its sketches. */
  sectionAt: (adjusted: boolean, station: string, fillGaps: boolean, filter = '{}'): StationSection | null => JSON.parse(section_at(adjusted, station, fillGaps, filter)),
  /** Adds a stroke drawn on a station's cross section; every edit goes through the document's one edit path. */
  addSectionSketch: (station: string, kind: string, points: [number, number][], widths: number[] | null, closed: boolean, adjusted: boolean, text = '', style = ''): string => add_section_sketch(station, kind, JSON.stringify(points), widths ? JSON.stringify(widths) : '', closed, adjusted, text, style),
  /** The cross-section sketches to show on the map. */
  mapSections: (adjusted: boolean, filter = '{}'): MapSection[] => JSON.parse(map_sections(adjusted, filter)),
  /** The profile sketches placed along their runs in 3D, for the map. */
  mapProfileSketches: (adjusted: boolean): ProfileSketch3d[] => JSON.parse(map_profile_sketches(adjusted)),
  /** Places a station's section sketches by hand in a view (an undoable edit). */
  placeSection: (station: string, view: 'map' | 'profile', offset: [number, number, number]): string => place_section(station, view, offset[0], offset[1], offset[2]),
  /** Adds a line of `kind` drawn in plan on the map (anchored to the survey, undoable). */
  addMapLine: (kind: string, points: [number, number][], widths: number[] | null, closed: boolean, adjusted: boolean, text = '', style = ''): string => add_map_line(kind, JSON.stringify(points), widths ? JSON.stringify(widths) : '', closed, adjusted, text, style),
  /** The id of the line most recently added (map, profile or section). */
  lastAddedId: (): string => last_added_id(),
  /** Pastes lines into a view as one undoable edit (`target` is the run or station, empty on the map): the new ids, or an error text. */
  pasteLines: (view: 'map' | 'profile' | 'section', target: string, adjusted: boolean, lines: { kind: string; points: number[][]; widths: number[] | null; closed: boolean; text: string; style: string }[]): { ids: string[] } | string => {
    const r = paste_lines(view, target, adjusted, JSON.stringify(lines));
    return r.startsWith('error:') ? r : (JSON.parse(r) as { ids: string[] });
  },
  /** Moves the fit points of a line already drawn (in the space it was drawn in); re-anchors it to the survey. */
  editLeader: (id: string, points: number[][], style: string, adjusted: boolean): string => edit_leader(id, JSON.stringify(points), style, adjusted),
  editLine: (id: string, points: [number, number][], widths: number[] | null, adjusted: boolean): string => edit_line(id, JSON.stringify(points), widths ? JSON.stringify(widths) : '', adjusted),
  /** Closes or opens a line already drawn. */
  setLineClosed: (id: string, closed: boolean): string => set_line_closed(id, closed),
  /** Changes what a line already drawn is (wall, ledge, ...). */
  setLineKind: (id: string, kind: string): string => set_line_kind(id, kind),
  /** What a drawn line is, or null when the id is not a drawn line. */
  setLineText: (id: string, text: string): string => set_line_text(id, text),
  setLineStyle: (id: string, style: string): string => set_line_style(id, style),
  setLineFlip: (id: string, flip: boolean): string => set_line_flip(id, flip),
  /** The papers and plot scales a sheet offers. */
  sheetOptions: (): SheetOptions => JSON.parse(sheet_options()),
  /** A sheet laid out for a cave this many metres wide and tall (see `sheet_plan` in the engine). */
  sheetPlan: (width: number, height: number, p: SheetSetup): SheetPlan => JSON.parse(sheet_plan(width, height, p.paper, p.turned, p.feetPerInch, p.name, JSON.stringify(p.lines), p.title, p.bar, p.north)),
  ornament: (kind: string, curve: [number, number][], closed: boolean, flip: boolean, id: string, survey: [number, number][]): { spine: boolean; lines: [number, number][][] } => JSON.parse(ornament(kind, JSON.stringify(curve), closed, flip, id, JSON.stringify(survey))),
  flippedLines: (): string[] => JSON.parse(flipped_lines()),
  lineInfo: (id: string): { kind: string; view: 'profile' | 'section' | 'map'; closed: boolean; flip: boolean; text: string; style: string } | null => JSON.parse(line_info(id)),
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
  addSketch: (run: string, kind: string, points: [number, number][], widths: number[] | null, closed: boolean, adjusted: boolean, text = '', style = ''): string => add_profile_sketch(run, kind, JSON.stringify(points), widths ? JSON.stringify(widths) : '', closed, adjusted, text, style),
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

/** The engine, which refuses every write while this person is only watching a class (see `readonly.ts`). */
export const engine = guardEngine(rawEngine);

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
setPatternSource(engine); // so are the marks of a custom linetype or fill

// Dev-only handle for tests and debugging in the browser console.
if (import.meta.env.DEV) (window as unknown as { __engine: typeof engine }).__engine = engine;
