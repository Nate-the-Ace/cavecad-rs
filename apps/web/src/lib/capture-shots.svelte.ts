// LiDAR rooms from CaveCAD Capture on the same iPhone or iPad, read straight from Capture's Files folder (linked
// once, remembered as a bookmark: src-tauri/src/capture.rs). Native code lists and places the open cave's shots
// the way Capture places them (card chaining first) and hands back each placed room's points, thinned and relative
// to its station; this keeps the state, the cache of points and the refresh (on open, on focus, when the stations
// change). Nothing is uploaded or written into the cave. Anywhere but CaveCAD on iPhone/iPad it never starts.

import { untrack } from 'svelte';
import { cave } from './cave.svelte';
import { engine } from './engine';
import { captureShare } from './capture-share.svelte';
import { captureLink, captureLinkAvailable, captureLinkState, captureShotPoints, captureShots as listShots, type ShotRow } from './desktop';
import { cellFor, colorFor, counts, pointsKey } from './capture-pure';

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

const cache = new Map<string, Float32Array>();
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
  const placed = rows.filter((r) => r.status === 'placed' && r.rigid && r.anchor);
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
