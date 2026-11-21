// LiDAR rooms from CaveCAD Capture on the same iPhone or iPad, read straight from Capture's Files folder (linked
// once, remembered as a bookmark: src-tauri/src/capture.rs). Native code lists and places the open cave's shots
// the way Capture places them (card chaining first) and hands back each placed room's points, thinned and relative
// to its station; this keeps the state, the cache of points and the refresh (on open, on focus, when the stations
// change). Nothing is uploaded or written into the cave. Anywhere but CaveCAD on iPhone/iPad it never starts.

import { untrack } from 'svelte';
import { cave } from './cave.svelte';
import { engine } from './engine';
import { captureShare } from './capture-share.svelte';
import { captureLink, captureLinkAvailable, captureLinkState, captureSection, captureShotPlanWalls, captureShotPoints, captureShotProfileWalls, captureShots as listShots, type PlacedShot, type ShotRow, type SliceLines } from './desktop';
import { cellFor, colorFor, counts, placeKey, pointsKey } from './capture-pure';
import type { BandAxis } from './engine';

/** One room ready to draw: its points relative to `anchor` (or to its station as the view shows it). */
export type CaptureLayer = { key: string; stationId: string | null; anchor: [number, number, number]; points: Float32Array; color: number };

export const captureShots = $state({
  /** CaveCAD on iPhone/iPad: the link exists. */
  available: false,
  /** Capture's shots folder can be read. */
  linked: false,
  /** The remembered folder went stale or no longer opens. */
  stale: false,
  /** What to tell the person, or ''. */
  problem: '',
  /** A refresh is running. */
  busy: false,
  rows: [] as ShotRow[],
  /** Rooms drawn, and points in all of them. */
  drawn: 0,
  points: 0,
});

let layers = $state.raw<CaptureLayer[]>([]);
/** The rooms to draw (raw: the point arrays are never proxied). */
export const captureRooms = {
  get layers(): CaptureLayer[] {
    return layers;
  },
};

/** One room's plan walls: its mesh cut level at its station's height, `[east, north]` metres from `anchor`. */
export type CaptureWalls = { key: string; stationId: string | null; anchor: [number, number, number]; lines: SliceLines; color: number };

let wallLayers = $state.raw<CaptureWalls[]>([]);
/** The rooms' plan walls to draw on the map. */
export const captureWalls = {
  get layers(): CaptureWalls[] {
    return wallLayers;
  },
};

const cache = new Map<string, Float32Array>();
const wallCache = new Map<string, SliceLines>();
const profileCache = new Map<string, SliceLines>();
const sectionCache = new Map<string, { lines: SliceLines; shots: number }>();
let lastWalls = '';
let lastProfile = '';
let lastSection = '';
let running = false;
let again = false;
let lastLink = '';
let lastList = '';
let lastPoints = '';

function clear() {
  captureShots.rows = [];
  captureShots.drawn = 0;
  captureShots.points = 0;
  if (layers.length) layers = [];
  if (wallLayers.length) wallLayers = [];
  // the same rooms coming back after a clear must be drawn (and logged) again
  lastPoints = '';
  lastWalls = '';
  lastProfile = '';
  lastSection = '';
  profileCache.clear();
  sectionCache.clear();
}

const placedOf = (r: ShotRow): PlacedShot => ({ file: r.file, rigid: r.rigid!, anchor: r.anchor! });
const isPlaced = (r: ShotRow) => r.status === 'placed' && !!r.rigid && !!r.anchor;

/** Plan walls for every placed room, each cut at its own station's height (cached per shot and placement). */
async function loadWalls(id: string, placed: ShotRow[]) {
  const next: CaptureWalls[] = [];
  const keep = new Set<string>();
  for (const r of placed) {
    const key = `${id}|${placeKey(r)}`;
    let lines = wallCache.get(key);
    if (!lines) {
      try {
        lines = await captureShotPlanWalls(id, placedOf(r), 0);
      } catch (e) {
        engine.log('warn', 'capture', `walls of ${r.label} (${r.file}) not cut: ${String(e)}`);
        continue;
      }
      wallCache.set(key, lines);
    }
    keep.add(key);
    next.push({ key, stationId: r.stationId, anchor: r.anchor!, lines, color: colorFor(r.shotId) });
  }
  for (const k of [...wallCache.keys()]) if (!keep.has(k)) wallCache.delete(k);
  const wk = next.map((w) => w.key).join(';');
  if (wk !== lastWalls) {
    lastWalls = wk;
    wallLayers = next;
    engine.log('info', 'capture', `LiDAR walls: ${next.length} shot${next.length === 1 ? '' : 's'} sliced, ${next.reduce((n, w) => n + w.lines.length, 0)} lines`);
  }
}

/** Profile walls on one run's band: every placed room whose station is on the band, cut upright along the band's ground
 *  direction at that station, as `[s, up]` metres from the station (place with `onBand`). Cached per shot, placement and direction. */
export async function profileWalls(run: string, axes: BandAxis[]): Promise<{ at: BandAxis; lines: SliceLines; color: number }[]> {
  const id = engine.roomId();
  if (!captureShots.linked || !id) return [];
  const byId = new Map(axes.map((a) => [a.id, a]));
  const out: { at: BandAxis; lines: SliceLines; color: number }[] = [];
  const keys: string[] = [];
  for (const r of captureShots.rows) {
    const at = r.stationId ? byId.get(r.stationId) : undefined;
    if (!isPlaced(r) || !at) continue;
    const key = `${id}|${placeKey(r)}|${at.along[0].toFixed(4)},${at.along[1].toFixed(4)}`;
    let lines = profileCache.get(key);
    if (!lines) {
      try {
        lines = await captureShotProfileWalls(id, placedOf(r), at.along);
      } catch (e) {
        engine.log('warn', 'capture', `profile walls of ${r.label} (${r.file}) not cut: ${String(e)}`);
        continue;
      }
      profileCache.set(key, lines);
    }
    keys.push(`${key}|${at.s.toFixed(3)},${at.z.toFixed(3)}`);
    out.push({ at, lines, color: colorFor(r.shotId) });
  }
  const pk = `${run}|${keys.join(';')}`;
  if (pk !== lastProfile) {
    lastProfile = pk;
    engine.log('info', 'capture', `LiDAR profile walls: ${out.length} shot${out.length === 1 ? '' : 's'}, ${out.reduce((n, w) => n + w.lines.length, 0)} lines (run ${run})`);
  }
  return out;
}

/** The LiDAR cross section at a station, in the section panel's own frame (`right`, `up`): `[across, up]` metres from the
 *  station, cut from every placed room that comes within 3 m of it. Null when there is nothing to cut from. */
export async function sectionCut(stationId: string, label: string, right: number[], up: number[]): Promise<{ lines: SliceLines; shots: number } | null> {
  const id = engine.roomId();
  if (!captureShots.linked || !id || !cave.hasCave) return null;
  const placed = captureShots.rows.filter(isPlaced);
  if (!placed.length) return null;
  const st = engine.captureStations(cave.name, 0)?.stations.find((s) => s.id === stationId);
  if (!st) return null;
  const at = [st.x, st.y, st.z];
  const key = `${id}|${stationId}|${at.map((v) => v.toFixed(3))}|${right.map((v) => v.toFixed(5))}|${up.map((v) => v.toFixed(5))}|${placed.map(placeKey).join(';')}`;
  let cut = sectionCache.get(key);
  if (!cut) {
    cut = await captureSection(id, placed.map(placedOf), at, right, up);
    if (sectionCache.size > 64) sectionCache.clear();
    sectionCache.set(key, cut);
  }
  if (key !== lastSection) {
    lastSection = key;
    engine.log('info', 'capture', `LiDAR section at ${label}: ${cut.lines.length} lines from ${cut.shots} shot${cut.shots === 1 ? '' : 's'}`);
  }
  return cut;
}

async function once() {
  const st = await captureLinkState();
  captureShots.linked = st.linked;
  captureShots.stale = st.stale;
  captureShots.problem = st.problem;
  const link = `${st.linked}|${st.stale}|${st.problem}`;
  if (link !== lastLink) {
    lastLink = link;
    engine.log('info', 'capture', `Capture folder: ${st.linked ? 'linked' : 'not linked'}${st.stale ? ' (stale)' : ''}${st.problem ? ` — ${st.problem}` : ''}`);
  }
  const id = engine.roomId();
  if (!st.linked || !cave.hasCave || !id) {
    clear();
    return;
  }
  const shared = engine.captureStations(cave.name, 0);
  const rows = await listShots(id, shared?.stations ?? []);
  captureShots.rows = rows;
  const c = counts(rows);
  const list = `${id}|${c.placed}|${c.waiting}|${c.gone}|${c.unreadable}`;
  if (list !== lastList) {
    lastList = list;
    engine.log('info', 'capture', `Capture shots: ${c.placed} placed, ${c.waiting} waiting for their station, ${c.gone} station gone${c.unreadable ? `, ${c.unreadable} unreadable` : ''}`);
  }
  const placed = rows.filter(isPlaced);
  const cell = cellFor(placed.reduce((n, r) => n + r.vertices, 0));
  const next: CaptureLayer[] = [];
  const keep = new Set<string>();
  for (const r of placed) {
    const key = `${id}|${pointsKey(r, cell)}`;
    let pts = cache.get(key);
    if (!pts) {
      try {
        pts = await captureShotPoints(id, r.file, r.rigid!, r.anchor!, cell);
      } catch (e) {
        engine.log('warn', 'capture', `room ${r.label} (${r.file}) not read: ${String(e)}`);
        continue;
      }
      cache.set(key, pts);
    }
    keep.add(key);
    next.push({ key, stationId: r.stationId, anchor: r.anchor!, points: pts, color: colorFor(r.shotId) });
  }
  for (const k of [...cache.keys()]) if (!keep.has(k)) cache.delete(k);
  const total = next.reduce((n, l) => n + l.points.length / 3, 0);
  captureShots.drawn = next.length;
  captureShots.points = total;
  const pk = `${next.map((l) => l.key).join(';')}`;
  if (pk !== lastPoints) {
    lastPoints = pk;
    layers = next;
    engine.log('info', 'capture', `LiDAR rooms loaded: ${next.length} rooms, ${total} points (cell ${Math.round(cell * 100)} cm)`);
  }
  await loadWalls(id, placed);
}

/** Reads the link and the shots again (one at a time; a call during a refresh runs once more after it). */
export async function refreshCaptureShots(): Promise<void> {
  if (!captureShots.available) return;
  if (running) {
    again = true;
    return;
  }
  running = true;
  captureShots.busy = true;
  try {
    do {
      again = false;
      await once();
    } while (again);
  } catch (e) {
    engine.log('warn', 'capture', `Capture shots not read: ${String(e)}`);
  } finally {
    running = false;
    captureShots.busy = false;
  }
}

/** Shows the Files picker to link Capture's folder, then reads the shots. */
export async function linkCaptureShots(): Promise<void> {
  try {
    const st = await captureLink();
    captureShots.problem = st.problem;
  } catch (e) {
    captureShots.problem = String(e);
    engine.log('warn', 'capture', `Capture folder not linked: ${String(e)}`);
    return;
  }
  await refreshCaptureShots();
}

/** Starts reading Capture's shots (CaveCAD on iPhone and iPad only). Call once the engine is loaded. */
export async function startCaptureShots(): Promise<void> {
  if (!(await captureLinkAvailable())) return;
  captureShots.available = true;
  const soon = () => void refreshCaptureShots();
  document.addEventListener('visibilitychange', () => {
    if (document.visibilityState === 'visible') soon();
  });
  window.addEventListener('focus', soon);
  $effect.root(() => {
    $effect(() => {
      void cave.opened;
      void cave.hasCave;
      void captureShare.text; // the stations moved (published 2 s after an edit)
      untrack(soon);
    });
  });
}
