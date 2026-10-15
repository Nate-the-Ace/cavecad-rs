// Object snap, as the screen holds it: the settings and what is switched on, the state of the pick in progress, the latest
// result for the overlay, and the one-pick menu. What snapping FINDS is Rust (`cave-snap`, through `engine.snap*`); this file
// keeps what a person has chosen (remembered on this machine, every access guarded) and hands scenes to the engine.

import { engine, loadEngine } from './engine';
import type { SnapAcquired, SnapCandidate, SnapMode, SnapModeInfo, SnapPrefs, SnapResult, SnapScene, SnapSession, SnapSettings, V3 } from './snap-types';

const KEY = 'cavecad.snap.v1';

const FALLBACK: SnapSettings = { modes: ['endpoint', 'midpoint', 'center', 'quadrant', 'intersection', 'station', 'insertion'], aperture: 10, polar: [15, 30, 45, 90], tracking: true, intersection_tol: 0.05 };

export const snap = $state({
  /** Running snaps on (F3, the Snap button). */
  on: true,
  settings: { ...FALLBACK } as SnapSettings,
  prefs: { marker: true, tooltip: true, magnet: true } as SnapPrefs,
  /** The modes the settings list and the menu offer. */
  catalog: [] as SnapModeInfo[],
  /** The pick in progress. */
  session: {} as SnapSession,
  /** The latest result, for the overlay. */
  result: null as SnapResult | null,
  /** Which candidate Tab has chosen (0 is the best). */
  tab: 0,
  /** The one-pick override menu, where it is open (client pixels). */
  menu: null as { x: number; y: number } | null,
  /** The settings popover is open. */
  settingsOpen: false,
  /** The next pick is a free one: no snapping (the menu's None). */
  free: false,
});

function save() {
  try {
    localStorage.setItem(KEY, JSON.stringify({ on: snap.on, settings: snap.settings, prefs: snap.prefs }));
  } catch {
    /* settings just are not remembered */
  }
}

/** Reads the catalog from the engine and what was remembered. Safe to call more than once. */
export async function initSnap() {
  await loadEngine();
  const c = engine.snapCatalog();
  snap.catalog = c.modes;
  snap.settings = { ...c.settings };
  try {
    const kept = JSON.parse(localStorage.getItem(KEY) ?? 'null');
    if (kept && typeof kept === 'object') {
      if (typeof kept.on === 'boolean') snap.on = kept.on;
      const s = kept.settings;
      if (s && Array.isArray(s.modes)) {
        const known = new Set(c.modes.map((m) => m.id));
        snap.settings = {
          modes: s.modes.filter((m: SnapMode) => known.has(m)),
          aperture: typeof s.aperture === 'number' && s.aperture >= 3 && s.aperture <= 40 ? s.aperture : c.settings.aperture,
          polar: Array.isArray(s.polar) && s.polar.every((n: unknown) => typeof n === 'number') ? s.polar : c.settings.polar,
          tracking: typeof s.tracking === 'boolean' ? s.tracking : c.settings.tracking,
          intersection_tol: c.settings.intersection_tol,
        };
      }
      const p = kept.prefs;
      if (p && typeof p === 'object') snap.prefs = { marker: p.marker !== false, tooltip: p.tooltip !== false, magnet: p.magnet !== false };
    }
  } catch {
    /* nothing remembered, or storage unavailable */
  }
}

/** Switches the running snaps on or off. */
export function toggleSnap() {
  snap.on = !snap.on;
  if (!snap.on) clearPick();
  save();
}

export function setMode(id: SnapMode, on: boolean) {
  const modes = new Set(snap.settings.modes);
  if (on) modes.add(id);
  else modes.delete(id);
  snap.settings = { ...snap.settings, modes: [...modes] };
  save();
}

export function setSetting<K extends 'aperture' | 'tracking' | 'polar'>(key: K, value: SnapSettings[K]) {
  snap.settings = { ...snap.settings, [key]: value };
  save();
}

export function setPref(key: keyof SnapPrefs, on: boolean) {
  snap.prefs = { ...snap.prefs, [key]: on };
  save();
}

/** True when a pick should be snapped: running snaps are on, or a one-pick override is waiting. */
export const snapping = (): boolean => !snap.free && (snap.on || !!snap.session.override);

/** What the engine is told: with the running snaps off only a one-pick override (or the references) does anything. */
function effective(): SnapSettings {
  return snap.on ? snap.settings : { ...snap.settings, modes: [], tracking: false };
}

/** What the cursor (pixels) can snap to now. Remembers the result for the overlay. */
export function query(scene: SnapScene, x: number, y: number): SnapResult | null {
  if (!snapping()) {
    snap.result = null;
    return null;
  }
  const r = engine.snapPick(scene, x, y, effective(), snap.session);
  // A different best candidate starts Tab again from the best.
  const before = snap.result?.best;
  if (!r.best || !before || before.mode !== r.best.mode || before.world.some((v, i) => Math.abs(v - r.best!.world[i]) > 1e-9)) snap.tab = 0;
  snap.result = r;
  return r;
}

/** The candidate a pick would use: the best, or the one Tab has moved to. */
export function chosen(): SnapCandidate | null {
  const r = snap.result;
  if (!r?.best) return null;
  const all = [r.best, ...r.others];
  return all[snap.tab % all.length];
}

/** Tab: the next candidate. */
export function cycle() {
  const r = snap.result;
  if (r?.best) snap.tab = (snap.tab + 1) % (r.others.length + 1);
}

/** A pick landed on `world`: updates the session and answers the point to use (null when the pick only set something up). */
export function commit(world: V3): V3 | null {
  snap.free = false;
  const out = engine.snapCommit(snap.session, world);
  snap.session = out.session;
  snap.result = null;
  snap.tab = 0;
  return out.point;
}

/** The next pick searches only `mode` (or sets the reference up). */
export function onePick(mode: SnapMode | null) {
  snap.session = engine.snapOnePick(snap.session, mode);
  snap.menu = null;
}

/** The next pick is free: nothing is snapped to for it (the menu's None). */
export function freePick() {
  snap.free = true;
  snap.result = null;
  snap.menu = null;
}

/** The previous point of the pick in progress (for Perpendicular, Tangent and Parallel). */
export function setLast(p: V3 | null) {
  snap.session = engine.snapLastPoint(snap.session, p);
}

/** Hovered long enough: acquires what is under the cursor (an end to extend from, a segment to run parallel to, a point to track from). */
export function acquireHover() {
  const r = snap.result;
  if (!r?.acquirable.length || !snapping()) return;
  const pick: SnapAcquired = r.acquirable.find((a) => a.type === 'extension') ?? r.acquirable.find((a) => a.type === 'track') ?? r.acquirable[0];
  snap.session = engine.snapAcquire(snap.session, pick);
}

/** Escape: drops the track points, acquired points and override a pick built up (the tool's own last point stays). */
export function escapePick() {
  snap.session = engine.snapCancel(snap.session);
  snap.result = null;
  snap.tab = 0;
  snap.menu = null;
}

/** True when there is anything for Escape to drop. */
export const pickBuiltUp = (): boolean => {
  const s = snap.session;
  return !!(s.acquired?.length || s.track_points?.length || s.from_base || s.m2p_first || s.override);
};

/** The tool ended or changed: drops everything, the last point too. */
export function clearPick() {
  snap.free = false;
  snap.session = engine.snapCancel({ ...snap.session, last_point: null });
  snap.result = null;
  snap.tab = 0;
  snap.menu = null;
}

/** The name of the one-pick override in force, for the "No ... here" note. */
export function overrideName(): string {
  const o = snap.session.override;
  return o ? (snap.catalog.find((m) => m.id === o)?.name ?? '') : '';
}
