// State every panel shares about the open cave. The cave itself lives in the Rust engine; this
// only says "it changed, look again" so panels refresh together.

import type { Units } from './units';
import { engine } from './engine';
import { setHiddenTest } from './section-glyph';

const UNITS_KEY = 'cavecad.units';

function savedUnits(): Units {
  try {
    return localStorage.getItem(UNITS_KEY) === 'imperial' ? 'imperial' : 'metric';
  } catch {
    return 'metric';
  }
}

const OPTIONS_KEY = 'cavecad.options';

function savedOptions(): Record<string, number | boolean> {
  try {
    const raw = JSON.parse(localStorage.getItem(OPTIONS_KEY) ?? 'null');
    if (raw && typeof raw === 'object') return raw;
  } catch {
    // nothing saved
  }
  return {};
}

const SECTIONS_KEY = 'cavecad.sections';

function savedSections(): { map: boolean; profile: boolean; profileSketches: boolean } {
  try {
    const raw = JSON.parse(localStorage.getItem(SECTIONS_KEY) ?? 'null');
    if (raw) return { map: raw.map !== false, profile: raw.profile !== false, profileSketches: raw.profileSketches !== false };
  } catch {
    // nothing saved
  }
  return { map: true, profile: true, profileSketches: true };
}

export const cave = $state({
  /** Settings of running commands (curve detail, pen pressure, ...), remembered on this machine. */
  options: savedOptions() as Record<string, number | boolean | string>,
  /** The drawing tool picked on the Draw tab (a palette type such as `wall`), or empty. While one is
   *  picked, drawing in any view makes linework of that type in that view. */
  tool: '',
  /** The line (a feature id) selected for editing, or empty. Its fit points show as handles in the view it was drawn in. */
  selected: '',
  /** Whether cross-section sketches are drawn beside their stations, per view, and whether profile
   *  sketches are drawn along their runs on the map (view settings). */
  showSections: savedSections(),
  /** What lengths are shown in (the cave itself is always metres). Metric unless changed. */
  units: savedUnits(),
  /** Bumps whenever a cave is opened or edited. Panels re-read when it changes. */
  version: 0,
  /** Show least-squares adjusted positions. */
  adjusted: true,
  /** Bumps when a whole cave is opened from a file, so the canvas re-reads and re-frames. */
  opened: 0,
  /** Name of the file the cave came from or will be saved to. */
  fileName: 'cave.dxf',
  /** The station (id) the Cross Section panel is showing; empty until one is picked. */
  station: '',
  /** The run the Profile panel is showing. */
  run: '',
});

/** What the canvas hides, by type and by trip. A view setting only: nothing is stored on the drawing. */
export const filter = $state({ hiddenTypes: [] as string[], hiddenTrips: [] as string[] });

/** The filter in the shape the engine reads. */
export function wireFilter(): string {
  return JSON.stringify({ hidden_types: filter.hiddenTypes, hidden_trips: filter.hiddenTrips });
}

/** Shows or hides one type (`on` = shown) or one trip. */
export function setShown(kind: 'types' | 'trips', key: string, on: boolean) {
  const list = kind === 'types' ? filter.hiddenTypes : filter.hiddenTrips;
  const next = list.filter((k) => k !== key);
  if (!on) next.push(key);
  if (kind === 'types') filter.hiddenTypes = next;
  else filter.hiddenTrips = next;
  cave.version += 1; // the panels redraw without what is hidden
}

// strokes in the Profile, Cross Section and Map skip what the Show menu has hidden
setHiddenTest((kind) => filter.hiddenTypes.includes(kind));

/** Shows or hides the cross-section sketches in one view, and remembers it on this machine. */
export function setShowSections(view: 'map' | 'profile' | 'profileSketches', on: boolean) {
  cave.showSections[view] = on;
  try {
    localStorage.setItem(SECTIONS_KEY, JSON.stringify($state.snapshot(cave.showSections)));
  } catch {
    // private window
  }
}

/** Switches the display units and remembers the choice on this machine. */
export function setUnits(u: Units) {
  cave.units = u;
  try {
    localStorage.setItem(UNITS_KEY, u);
  } catch {
    // private window: the choice just lasts until the page closes
  }
}

/** Changes one option of the running command and remembers it. */
export function setOption(id: string, value: number | boolean | string) {
  cave.options[id] = value;
  try {
    // Words (a label to put on each symbol) last only until the page closes: a forgotten one would
    // quietly go on every symbol placed tomorrow.
    const keep = Object.fromEntries(Object.entries($state.snapshot(cave.options)).filter(([, v]) => typeof v !== 'string'));
    localStorage.setItem(OPTIONS_KEY, JSON.stringify(keep));
  } catch {
    // private window
  }
}

/** The current value of an option (its default until changed). */
export function option<T extends number | boolean | string>(id: string, fallback: T): T {
  const v = cave.options[id];
  return (typeof v === typeof fallback ? v : fallback) as T;
}

/** Picks a drawing tool; picking the one already active puts it down. */
export function setTool(key: string) {
  cave.tool = cave.tool === key ? '' : key;
}

/** The cave's revision at the last open or save: what "nothing unsaved" means. */
let savedRevision = 0;

/** Says the cave on screen is the one just opened, loaded or saved: nothing is unsaved now. */
export function markSaved() {
  try {
    savedRevision = engine.revision();
  } catch {
    savedRevision = 0; // the engine is not loaded yet
  }
  cave.version += 1; // whatever shows "unsaved" re-reads
}

/** True when the cave has changed since it was last opened or saved. Reads `cave.version`, so it is reactive. */
export function unsaved(): boolean {
  void cave.version;
  try {
    return engine.revision() !== savedRevision;
  } catch {
    return false;
  }
}

export function changed() {
  cave.version += 1;
}
