// Surface Data: the cave's surface (aerial photo + ground elevation) as this screen holds it, and the steps
// that change it. The record of what was downloaded lives in the cave (the engine's `surface` setting); the
// files themselves live in OPFS (surface-files.ts) and travel inside the .cavecad file.
// Coordinates and URLs are never logged: they locate the cave.

import { engine, type GeoStatus, type SurfaceLayers, type SurfacePlan, type SurfaceRecord } from './engine';
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

let photoBytes: Uint8Array | null = null;

function setPhoto(bytes: Uint8Array | null) {
  if (surface.photoUrl) URL.revokeObjectURL(surface.photoUrl);
  photoBytes = bytes;
  surface.photoUrl = bytes ? URL.createObjectURL(new Blob([bytes as BlobPart], { type: 'image/jpeg' })) : '';
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
  const s = storedRecord();
  if (!s) {
    setPhoto(null);
    surface.tiff = null;
    surface.version++;
    return [];
  }
  const plan = planOf(s);
  const missing: Part[] = [];
  const p = s.photo ? await load(requestKey(plan, 'photo')) : null;
  const e = s.elevation ? await load(requestKey(plan, 'elevation')) : null;
  if (gen !== generation) return []; // a newer reload overtook this one
  if (s.photo && !p) missing.push('photo');
  if (s.elevation && !e) missing.push('elevation');
  setPhoto(p);
  surface.tiff = e;
  surface.version++;
  return missing;
}

/** Reloads, and downloads again any part the record lists but this device does not have (a cave synced without its files). */
export async function syncSurface(): Promise<string | null> {
  if (surface.busy) return null;
  const missing = await reloadSurface();
  if (!missing.length) return null;
  const r = await fetchSurface({ photo: missing.includes('photo'), elevation: missing.includes('elevation') });
  const message = r.ok ? 'The surface for this cave was downloaded again on this device.' : `The surface for this cave could not be downloaded again on this device. ${r.lines.join(' ')}`;
  engine.log(r.ok ? 'info' : 'warn', 'surface', message); // no coordinates in here
  return message;
}

/** What a failed download says, in words a person can act on. */
function failure(e: unknown): string {
  const err = e as Error;
  if (err?.name === 'TypeError') return 'could not reach the USGS service. Check the internet connection and try again.';
  return err?.message ?? String(e);
}

/** Downloads what is asked for, keeps it, records it. Returns the report lines. */
export async function fetchSurface(want: { photo: boolean; elevation: boolean }): Promise<{ ok: boolean; lines: string[] }> {
  if (surface.busy) return { ok: false, lines: ['A download is already running.'] };
  if (!want.photo && !want.elevation) return { ok: false, lines: ['Nothing to download: tick the aerial photo, the ground elevation, or both.'] };
  const plan = engine.surfacePlan(want.photo, want.elevation, cave.adjusted);
  if ('error' in plan) return { ok: false, lines: [plan.error] };
  surface.busy = true;
  generation++; // a reload still running must not overwrite what this download puts on screen
  const lines: string[] = [];
  let ground = NaN;
  let photo: Uint8Array | null = null;
  let tiff: Uint8Array | null = null;
  try {
    if (plan.photo) {
      try {
        photo = await download(plan, 'photo');
        await store(requestKey(plan, 'photo'), photo);
        lines.push(`Aerial photo: ${plan.photo.w} x ${plan.photo.h} pixels, about ${Math.round(plan.bounds.xmax - plan.bounds.xmin)} m across.`);
      } catch (e) {
        lines.push(`Aerial photo: ${failure(e)}`);
        delete plan.photo;
      }
    }
    if (plan.elevation) {
      try {
        tiff = await download(plan, 'elevation');
        ground = engine.surfaceGround(plan, tiff);
        await store(requestKey(plan, 'elevation'), tiff);
        lines.push(Number.isFinite(ground) ? `Ground elevation: ground at the entrance ${formatLength(ground, cave.units, 1)} (NAVD88).` : 'Ground elevation: the service has no elevation data at the entrance. This place is probably outside 3DEP coverage (the United States).');
      } catch (e) {
        lines.push(`Ground elevation: ${failure(e)}`);
        delete plan.elevation;
      }
    }
    if (!plan.photo && !plan.elevation) return { ok: false, lines };
    const r = engine.surfaceCommit(plan, ground, today());
    if (r !== 'ok') lines.push(r.replace(/^error: /, ''));
    changed();
    // The record decides what is shown: a kept part whose window changed was dropped by the commit.
    const s = storedRecord();
    if (r === 'ok') {
      if (photo) setPhoto(photo);
      if (tiff) surface.tiff = tiff;
    }
    if (!s?.photo && photoBytes) {
      setPhoto(null);
      if (!want.photo) lines.push('Aerial photo: the earlier one no longer fits the new window, so it was dropped. Download it again to see it.');
    }
    if (!s?.elevation && surface.tiff) {
      surface.tiff = null;
      if (!want.elevation) lines.push('Ground elevation: the earlier one no longer fits the new window, so it was dropped. Download it again to see it.');
    }
    surface.version++;
    return { ok: r === 'ok', lines };
  } finally {
    surface.busy = false;
  }
}

/**
 * Rebuilds what the views draw from the kept files (no network): the photo's place, the contours in
 * the display unit, and the terrain. A result equal to the last one is kept as it was, so the views
 * redraw nothing when an edit far from the entrance changed nothing here.
 */
export function recomputeLayers() {
  const st = geoStatus();
  if (!st?.surface || !st.location) {
    surface.layers = null;
    surface.terrain = null;
    return;
  }
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

/** What the last entrance pick put in the settings history (its edits, and the state they left). */
let lastPick: { steps: number; after: string } | null = null;

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
  const res = engine.geoSetLocation({ station: st.location.station, lat: hit.lat, lon: hit.lon, ground_m: hit.ground_m, source: 'surface-pick' });
  if (res !== 'ok') return res.replace(/^error: /, '');
  changed();
  const moved = geoKey();
  const where = `the lowest ground in the circle (${formatLength(hit.ground_m, cave.units, 1)})`;
  if (moved === before) return `The entrance is already at ${where}.`;
  const had = st.surface;
  const again = await fetchSurface({ photo: !!had?.photo, elevation: true });
  const after = geoKey();
  // the move is one edit; a download that was recorded is a second one, undone with it
  lastPick = { steps: after === moved ? 1 : 2, after };
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
  let undone = 0;
  for (let i = 0; i < p.steps; i++) if (engine.undo('settings')) undone++;
  changed();
  return undone === p.steps ? 'The entrance is back where it was, with its surface.' : 'Only part of that pick could be undone.';
}

/** The parts for saving into the .cavecad file. */
export async function currentSurfaceParts(): Promise<SurfaceParts | null> {
  const s = storedRecord();
  if (!s) return null;
  return { record: JSON.stringify(s), photo: s.photo ? photoBytes : null, elevation: s.elevation ? surface.tiff : null };
}

/** Takes the surface files out of an opened .cavecad file. */
export async function adoptSurfaceParts(parts: SurfaceParts): Promise<void> {
  const s = storedRecord();
  if (!s) return;
  const plan = planOf(s);
  if (parts.photo && s.photo) await store(requestKey(plan, 'photo'), parts.photo);
  if (parts.elevation && s.elevation) await store(requestKey(plan, 'elevation'), parts.elevation);
  await reloadSurface();
}

setSurfacePartsSource(currentSurfaceParts);
setSurfaceAdopter(adoptSurfaceParts);
