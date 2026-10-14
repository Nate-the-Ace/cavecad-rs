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

/** What can be picked in a view to get a tab of its own in the ribbon (see `inspectors.ts`). */
export type PickKind = '' | 'station' | 'leg' | 'splay' | 'halo';

/** Picks a thing (a tool and a selected line are put down: one thing is picked at a time). */
export function setPick(kind: PickKind, id: string) {
  cave.pick = { kind, id: kind ? id : '' };
  if (kind) cave.selected = '';
}

/** Lets go of whatever is picked. */
export function clearPick() {
  cave.pick = { kind: '', id: '' };
}

const COMPACT_KEY = 'cavecad.ribbon.compact';

function savedCompact(): boolean {
  try {
    return localStorage.getItem(COMPACT_KEY) === '1';
  } catch {
    return false;
  }
}

/** Sets whether the ribbon folds every group into a dropdown (remembered on this machine). */
export function setCompactRibbon(on: boolean) {
  cave.compactRibbon = on;
  try {
    localStorage.setItem(COMPACT_KEY, on ? '1' : '0');
  } catch {
    // not remembered
  }
}

export const cave = $state({
  /** The ribbon folds every group into a dropdown, whatever the width. */
  compactRibbon: savedCompact(),
  /** Settings of running commands (curve detail, pen pressure, ...), remembered on this machine. */
  options: savedOptions() as Record<string, number | boolean | string>,
  /** The drawing tool picked on the Draw tab (a palette type such as `wall`), or empty. While one is
   *  picked, drawing in any view makes linework of that type in that view. */
  tool: '',
  /** The survey thing picked for its tab in the ribbon: a station, a leg or a splay (`kind`), by id. */
  pick: { kind: '' as PickKind, id: '' },
  /** The line (a feature id) selected for editing, or empty. Its fit points show as handles in the view it was drawn in. */
  selected: '',
  /** A leader waiting to gain or lose an arrow at the next press in a view (`add` touches the pressed spot, `remove` takes the arrow pressed on). */
  arrowOp: { id: '', op: '' as '' | 'add' | 'remove' },
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
  /** The New cave dialog is open. */
  newDialog: false,
  /** A cave is open (false on a clean slate, before one is chosen from the Cave Shelf). */
  hasCave: false,
  /** The open cave's name, for the title of the map. */
  name: '',
  /** A station a new trip should start from (picked on the Cave Shelf), used once by the Notebook. */
  tripFrom: '',
  /** The trip (id) the Notebook and the Scans panel are both on, so a page and its scans stay together. */
  trip: '',
  /** A request to bring the Map to a station (from the Stations panel). */
  focus: { id: '', seq: 0 },
  /** A request to fit the selected line again at another curve detail; the panel that holds the line does it. */
  refit: { id: '', detail: 5, seq: 0 },
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

/** The rock halo's settings, shared by every view: how far it reaches (metres) and how it fades. */
export const haloReach = () => option('haloReach', 6);
export const haloFade = () => option('haloFade', 0.5);

/** Picks a drawing tool; picking the one already active puts it down. */
/** The style a new stroke of the armed tool starts with: `pat:Name` for a fill of the caver's own, else none. */
export function toolStyle(): string {
  const name = String(cave.options.customFill ?? '');
  return cave.tool === 'fill-custom' && name ? `pat:${name}` : '';
}

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

/** Says the cave on screen has never been saved (a cave just started): it counts as unsaved from the first moment. */
export function markUnsaved() {
  savedRevision = -1;
  cave.version += 1;
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

/** Asks the Map to centre on a station. */
export function focusStation(id: string) {
  cave.focus = { id, seq: cave.focus.seq + 1 };
}

/** Asks for a drawn line to be fitted again at another curve detail (1 loose to 10 close). */
export function requestRefit(id: string, detail: number) {
  cave.refit = { id, detail, seq: cave.refit.seq + 1 };
}

export function changed() {
  cave.version += 1;
}

// Dev-only handle for tests and debugging in the browser console.
if (import.meta.env.DEV) (window as unknown as { __cave: unknown }).__cave = { cave, changed };
