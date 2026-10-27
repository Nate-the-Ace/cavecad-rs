// Surface Data: the cave's surface (aerial photo + ground elevation) as this screen holds it, and the steps
// that change it. The record of what was downloaded lives in the cave (the engine's `surface` setting); the
// files themselves live in OPFS (surface-files.ts) and travel inside the .cavecad file.
// Coordinates and URLs are never logged: they locate the cave.

import { engine, type GeoStatus, type Scope, type SurfaceLayers, type SurfacePlan, type SurfaceRecord } from './engine';
import { download, load, requestKey, store, type Part } from './surface-files';
import type { SurfaceParts } from './cavefile';
import { cave, changed } from './cave.svelte';
import { setSurfaceAdopter, setSurfacePartsSource } from './files';
import { FOOT, formatLength, inUnits } from './units';
import { sameTerrain, type Terrain } from './terrain';

// A class so the big things are held raw: contours and the terrain grid are replaced whole, never
// edited in place, and deep proxies over thousands of points would make every draw slow.
class SurfaceState {
  /** Object URL of the photo, or ''. */
  photoUrl = $state('');
  /** The elevation TIFF bytes, or null. */
  tiff = $state.raw<Uint8Array | null>(null);
  /** What the views draw (recomputed on cave/interval/unit change). */
  layers = $state.raw<SurfaceLayers | null>(null);
  /** The ground as a grid in the cave frame (NaN z = hole), or null with no elevation or no datum. */
  terrain = $state.raw<Terrain | null>(null);
  /** Bumps when the files change. */
  version = $state(0);
  /** A download is running. */
  busy = $state(false);
  /** The Surface Data dialog is open. */
  dialog = $state(false);
}

export const surface = new SurfaceState();

/**
 * "Pick entrance on surface": the Map's pointer mode (wired in Viewport3D), what the last pick said,
 * and whether that pick can still be undone from the message.
 */
export const entrancePick = $state({ on: false, message: '', canUndo: false });

/** Leaves pick mode and clears its message (the cave it was for is gone, or the ground it needs is). */
export function resetEntrancePick() {
  entrancePick.on = false;
  entrancePick.message = '';
  entrancePick.canUndo = false;
}

let photoBytes: Uint8Array | null = null;

const nextFrame = (f: () => void) => (typeof requestAnimationFrame === 'function' ? requestAnimationFrame(f) : setTimeout(f, 16));

/** The store key each part on screen was loaded or kept under ('' when none), so a save can tell the bytes match the record. */
const shownKey: Record<Part, string> = { photo: '', elevation: '' };

function setTiff(bytes: Uint8Array | null, key = '') {
  surface.tiff = bytes;
  shownKey.elevation = bytes ? key : '';
}

function setPhoto(bytes: Uint8Array | null, key = '') {
  shownKey.photo = bytes ? key : '';
  const old = surface.photoUrl;
  photoBytes = bytes;
  surface.photoUrl = bytes ? URL.createObjectURL(new Blob([bytes as BlobPart], { type: 'image/jpeg' })) : '';
  // Revoked a frame later, not now: the view may still be reading the old picture when the new URL lands.
  if (old) nextFrame(() => URL.revokeObjectURL(old));
}

const today = () => new Date().toISOString().slice(0, 10);

/** The open cave's geo status, or null with no cave open. */
export function geoStatus(): GeoStatus | null {
  const s = engine.geoStatus();
  return 'error' in s ? null : s;
}

/** The stored surface record, or null. */
function storedRecord(): SurfaceRecord | null {
  return geoStatus()?.surface ?? null;
}

/** The record as a plan (for the OPFS keys: they are made from the request). */
function planOf(s: SurfaceRecord): Pick<SurfacePlan, 'zone' | 'bounds'> & Partial<SurfacePlan> {
  return { zone: s.zone, bounds: s.bounds, for_at: s.for_at, photo: s.photo ? { w: s.photo[0], h: s.photo[1], url: '' } : undefined, elevation: s.elevation ? { w: s.elevation[0], h: s.elevation[1], url: '' } : undefined };
}

/** The stored record as text, for noticing when it changes (open, undo, redo, another device). */
export function surfaceKey(): string {
  return JSON.stringify(storedRecord());
}

let generation = 0;

/** Loads the stored surface's files from OPFS (after open, undo, or a cave switch). Returns what is missing. */
export async function reloadSurface(): Promise<Part[]> {
  const gen = ++generation;
  const serial = engine.docSerial();
  const s = storedRecord();
  if (!s) {
    setPhoto(null);
    setTiff(null);
    surface.version++;
    return [];
  }
  const plan = planOf(s);
  const missing: Part[] = [];
  const p = s.photo ? await load(requestKey(plan, 'photo')) : null;
  const e = s.elevation ? await load(requestKey(plan, 'elevation')) : null;
  if (gen !== generation || engine.docSerial() !== serial) return []; // a newer reload overtook this one, or another cave was opened
  if (s.photo && !p) missing.push('photo');
  if (s.elevation && !e) missing.push('elevation');
  setPhoto(p, requestKey(plan, 'photo'));
  setTiff(e, requestKey(plan, 'elevation'));
  surface.version++;
  return missing;
}

/** A sync was asked for while a download was running: it runs again when that download ends. */
let pendingSync = false;

/** Ends a download: clears busy and runs a sync that waited for it. */
function downloadDone() {
  surface.busy = false;
  if (!pendingSync) return;
  pendingSync = false;
  void syncSurface();
}

/**
 * Reloads, and downloads again any part the record lists but this device does not have (a cave synced
 * without its files). Asks for exactly what the record lists (its window and sizes) and records nothing:
 * the cave is not changed by fetching files it already describes. While another download runs it waits
 * and runs when that one ends, so a cave opened mid-download is never left without its files.
 */
export async function syncSurface(): Promise<string | null> {
  if (surface.busy) {
    pendingSync = true;
    return null;
  }
  const opened = cave.opened;
  const serial = engine.docSerial();
  const key = surfaceKey();
  const missing = await reloadSurface();
  if (!missing.length || cave.opened !== opened || engine.docSerial() !== serial || surfaceKey() !== key) return null;
  if (surface.busy) {
    pendingSync = true; // a download started while the files were read
    return null;
  }
  const plan = engine.surfacePlanForRecord(cave.adjusted);
  if ('error' in plan) return null;
  surface.busy = true;
  const gen = ++generation; // a reload still running must not overwrite what this puts on screen
  const stale = () => cave.opened !== opened || engine.docSerial() !== serial || surfaceKey() !== key || gen !== generation;
  let failed = false;
  try {
    for (const part of missing) {
      let bytes: Uint8Array;
      try {
        bytes = await download(plan, part);
      } catch {
        failed = true;
        continue;
      }
      if (stale()) return null;
      await store(requestKey(plan, part), bytes);
      if (stale()) return null;
      if (part === 'photo') setPhoto(bytes, requestKey(plan, part));
      else setTiff(bytes, requestKey(plan, part));
      surface.version++;
    }
  } finally {
    downloadDone();
  }
  const message = failed ? 'The surface for this cave could not be downloaded again on this device.' : 'The surface for this cave was downloaded again on this device.';
  engine.log(failed ? 'warn' : 'info', 'surface', message); // a fixed string: no coordinates, no URLs
  return message;
}

/** What a failed download says, in words a person can act on. */
function failure(e: unknown): string {
  const err = e as Error;
  if (err?.name === 'TypeError') return 'could not reach the USGS service. Check the internet connection and try again.';
  return err?.message ?? String(e);
}

/** What a download says when the cave on screen changed under it (nothing was recorded or shown). */
const CAVE_CHANGED = 'The cave changed while downloading.';

/**
 * Downloads what is asked for, keeps it, records it (one edit on `scope`'s history: `settings` from
 * the Surface Data dialog, `map` for an entrance pick). Returns the report lines. If another cave is
 * opened while it runs, it stops without recording or showing anything.
 */
export async function fetchSurface(want: { photo: boolean; elevation: boolean }, scope: Scope = 'settings'): Promise<{ ok: boolean; lines: string[] }> {
  if (surface.busy) return { ok: false, lines: ['A download is already running.'] };
  if (!want.photo && !want.elevation) return { ok: false, lines: ['Nothing to download: tick the aerial photo, the ground elevation, or both.'] };
  const plan = engine.surfacePlan(want.photo, want.elevation, cave.adjusted);
  if ('error' in plan) return { ok: false, lines: [plan.error] };
  surface.busy = true;
  generation++; // a reload still running must not overwrite what this download puts on screen
  const opened = cave.opened;
  const serial = engine.docSerial();
  const stale = () => cave.opened !== opened || engine.docSerial() !== serial;
  const gone = { ok: false, lines: [CAVE_CHANGED] };
  const lines: string[] = [];
  const sessionOnly = ' (This browser would not keep it on the device, so it lasts until the page closes; saving the cave keeps it in the file.)';
  let ground = NaN;
  let photo: Uint8Array | null = null;
  let tiff: Uint8Array | null = null;
  let photoKey = '';
  let tiffKey = '';
  try {
    if (plan.photo) {
      try {
        photo = await download(plan, 'photo');
      } catch (e) {
        lines.push(`Aerial photo: ${failure(e)}`);
        delete plan.photo;
      }
      if (stale()) return gone;
      if (photo && plan.photo) {
        photoKey = requestKey(plan, 'photo');
        const kept = await store(photoKey, photo);
        if (stale()) return gone;
        lines.push(`Aerial photo: ${plan.photo.w} x ${plan.photo.h} pixels, about ${Math.round(plan.bounds.xmax - plan.bounds.xmin)} m across.${kept ? '' : sessionOnly}`);
      }
    }
    if (plan.elevation) {
      try {
        tiff = await download(plan, 'elevation');
      } catch (e) {
        lines.push(`Ground elevation: ${failure(e)}`);
        delete plan.elevation;
      }
      if (stale()) return gone;
      if (tiff) {
        ground = engine.surfaceGround(plan, tiff);
        tiffKey = requestKey(plan, 'elevation');
        const kept = await store(tiffKey, tiff);
        if (stale()) return gone;
        lines.push((Number.isFinite(ground) ? `Ground elevation: ground at the entrance ${formatLength(ground, cave.units, 1)} (NAVD88).` : 'Ground elevation: the service has no elevation data at the entrance. This place is probably outside 3DEP coverage (the United States).') + (kept ? '' : sessionOnly));
      }
    }
    if (!plan.photo && !plan.elevation) return { ok: false, lines };
    const r = engine.surfaceCommit(plan, ground, today(), scope);
    if (r !== 'ok') lines.push(r.replace(/^error: /, ''));
    changed();
    // The record decides what is shown: a kept part whose window changed was dropped by the commit.
    const s = storedRecord();
    if (r === 'ok') {
      if (photo) setPhoto(photo, photoKey);
      if (tiff) setTiff(tiff, tiffKey);
    }
    if (!s?.photo && photoBytes) {
      setPhoto(null);
      if (!want.photo) lines.push('Aerial photo: the earlier one no longer fits the new window, so it was dropped. Download it again to see it.');
    }
    if (!s?.elevation && surface.tiff) {
      setTiff(null);
      if (!want.elevation) lines.push('Ground elevation: the earlier one no longer fits the new window, so it was dropped. Download it again to see it.');
    }
    surface.version++;
    return { ok: r === 'ok', lines };
  } finally {
    downloadDone();
  }
}

/**
 * Rebuilds what the views draw from the kept files (no network): the photo's place, the contours in
 * the display unit, and the terrain. A result equal to the last one is kept as it was, so the views
 * redraw nothing when an edit far from the entrance changed nothing here.
 */
let layersKey = '';
export function recomputeLayers() {
  const st = geoStatus();
  if (!st?.surface || !st.location) {
    layersKey = '';
    surface.layers = null;
    surface.terrain = null;
    return;
  }
  // Cheap key first: an edit far from the entrance changes none of these, and then nothing is rebuilt.
  const key = JSON.stringify([st.location, st.surface, st.interval_m, cave.units, cave.adjusted, engine.geoEntranceXyz(cave.adjusted), surface.version]);
  if (key === layersKey) return;
  layersKey = key;
  const u = cave.units;
  const feet = u === 'imperial';
  const intervalM = st.interval_m ?? (feet ? 10 * FOOT : 5);
  const interval = Math.round(inUnits(intervalM, u) * 1000) / 1000;
  const l = engine.surfaceLayers(surface.tiff ?? new Uint8Array(), interval, feet, cave.adjusted);
  const layers = 'error' in l ? null : l;
  if (JSON.stringify(layers) !== JSON.stringify(surface.layers)) surface.layers = layers;
  const terrain = surface.tiff ? engine.surfaceTerrain(surface.tiff, cave.adjusted) : null;
  if (!sameTerrain(terrain, surface.terrain)) surface.terrain = terrain;
}

/** The location and surface record as they stand, for telling whether anything else changed them since. */
function geoKey(): string {
  const st = geoStatus();
  return JSON.stringify([st?.location ?? null, st?.surface ?? null, st?.interval_m ?? null]);
}

/** What the last entrance pick put in the Map's history: its edits, the state they left, and the cave and revision after them. */
let lastPick: { steps: number; after: string; opened: number; revision: number } | null = null;

/** Whether the message's Undo can still take back the last pick (nothing has been edited, undone or opened since). */
export function pickUndoable(): boolean {
  const p = lastPick;
  return !!p && p.opened === cave.opened && p.revision === engine.revision() && geoKey() === p.after;
}

/**
 * Moves the entrance to the lowest ground inside the circle (cave metres on the plan), then downloads
 * the surface again for the new spot, without asking. Returns what to tell the person (no coordinates).
 */
export async function pickEntrance(cx: number, cy: number, r: number): Promise<string> {
  lastPick = null;
  entrancePick.canUndo = false;
  if (!surface.tiff) return 'Download the ground elevation first (Surface Data).';
  if (surface.busy) return 'A download is running: try again when it has finished.';
  const st = geoStatus();
  if (!st?.location) return 'Set the cave location first (Surface Data).';
  const hit = engine.surfacePick(surface.tiff, cx, cy, r, cave.adjusted);
  if ('error' in hit) return 'No ground data inside that circle.';
  const before = geoKey();
  const opened = cave.opened;
  const serial = engine.docSerial();
  // On the Map's history, so the Map's own Undo takes the pick back (the download first, then the move).
  const res = engine.geoSetLocation({ station: st.location.station, lat: hit.lat, lon: hit.lon, ground_m: hit.ground_m, source: 'surface-pick' }, 'map');
  if (res !== 'ok') return res.replace(/^error: /, '');
  changed();
  const moved = geoKey();
  const where = `the lowest ground in the circle (${formatLength(hit.ground_m, cave.units, 1)})`;
  if (moved === before) return `The entrance is already at ${where}.`;
  const had = st.surface;
  const again = await fetchSurface({ photo: !!had?.photo, elevation: true }, 'map');
  if (cave.opened !== opened || engine.docSerial() !== serial) return CAVE_CHANGED;
  const after = geoKey();
  // the move is one edit; a download that was recorded is a second one, undone with it
  lastPick = { steps: after === moved ? 1 : 2, after, opened, revision: engine.revision() };
  entrancePick.canUndo = true;
  if (!again.ok) return `The entrance moved, but the surface could not be downloaded again (${again.lines.join(' ')}). It still shows the old area; Surface Data will offer it again. It now sits at ${where}.`;
  const now = geoStatus()?.surface;
  const noPhoto = had?.photo && !now?.photo ? ' The aerial photo could not be downloaded again: Surface Data will offer it.' : '';
  return `The entrance moved to ${where}. The surface was downloaded again for the new spot.${noPhoto}`;
}

/**
 * Undoes the last entrance pick (the move, and the download it set off). The earlier files come back
 * from the device's store through the record (App.svelte reloads them when the record changes).
 * Refused when anything else changed the location or surface since.
 */
export function undoEntrancePick(): string {
  const p = lastPick;
  lastPick = null;
  entrancePick.canUndo = false;
  if (!p) return '';
  if (geoKey() !== p.after) return 'The location or surface changed again since that pick, so it was not undone.';
  if (p.opened !== cave.opened || p.revision !== engine.revision()) return 'The cave was edited since that pick, so it was not undone here. Undo on the Map steps back through it one edit at a time.';
  let undone = 0;
  for (let i = 0; i < p.steps; i++) if (engine.undo('map')) undone++;
  changed();
  return undone === p.steps ? 'The entrance is back where it was, with its surface.' : 'Only part of that pick could be undone.';
}

/** The parts for saving into the .cavecad file. */
export async function currentSurfaceParts(): Promise<SurfaceParts | null> {
  const s = storedRecord();
  if (!s) return null;
  // Loaded by the record's own keys, so a save never packs bytes that do not match the record; the bytes
  // on screen stand in only when they are the ones under that key (the store may not have kept them).
  const plan = planOf(s);
  const part = async (p: Part, shown: Uint8Array | null) => {
    const key = requestKey(plan, p);
    return (await load(key)) ?? (shown && shownKey[p] === key ? shown : null);
  };
  return { record: JSON.stringify(s), photo: s.photo ? await part('photo', photoBytes) : null, elevation: s.elevation ? await part('elevation', surface.tiff) : null };
}

/**
 * Takes the surface files out of an opened .cavecad file: shown straight from the file's parts, and kept
 * (memory for the session, OPFS where the browser allows) so a reload or undo finds them again.
 */
export async function adoptSurfaceParts(parts: SurfaceParts): Promise<void> {
  const s = storedRecord();
  if (!s) return;
  const gen = ++generation;
  const plan = planOf(s);
  const photo = s.photo ? parts.photo : null;
  const elevation = s.elevation ? parts.elevation : null;
  if (photo) await store(requestKey(plan, 'photo'), photo);
  if (elevation) await store(requestKey(plan, 'elevation'), elevation);
  const p = photo ?? (s.photo ? await load(requestKey(plan, 'photo')) : null);
  const e = elevation ?? (s.elevation ? await load(requestKey(plan, 'elevation')) : null);
  if (gen !== generation) return; // a newer reload or download overtook this one
  setPhoto(p, requestKey(plan, 'photo'));
  setTiff(e, requestKey(plan, 'elevation'));
  surface.version++;
}

setSurfacePartsSource(currentSurfaceParts);
setSurfaceAdopter(adoptSurfaceParts);
