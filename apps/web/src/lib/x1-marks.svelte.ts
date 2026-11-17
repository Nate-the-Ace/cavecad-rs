// What a shot the surveyor classed on the X1 (its M button while the laser is on: Feature, Ridge or Generic) draws on
// the map. The X1's classes are fixed by its firmware; what each one means here is the surveyor's choice, kept on this
// device: nothing (the splay is only coloured), a symbol placed where the shot hit, or a line through the hits of that
// class shot one after another from one station (a ledge along a ridge, say).

import { cave, changed } from './cave.svelte';
import { engine } from './engine';
import type { LogEntry } from './survey-types';

export type MarkClass = 'feature' | 'ridge' | 'generic';
export const MARK_CLASSES: { id: MarkClass; label: string; hint: string }[] = [
  { id: 'feature', label: 'Feature', hint: 'A point worth a symbol: a formation, a pit, a drip' },
  { id: 'ridge', label: 'Ridge', hint: 'Points along an edge: shot one after another from a station, they make one line' },
  { id: 'generic', label: 'Generic', hint: 'Whatever you choose' },
];

const KEY = 'cavecad.x1Marks';
/** '' draws nothing (the splay is coloured only); `sym-…` places that symbol; anything else is a line kind. */
const DEFAULT: Record<MarkClass, string> = { feature: '', ridge: 'ledge', generic: '' };

function load(): Record<MarkClass, string> {
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) ?? 'null');
    if (raw && typeof raw === 'object') return { ...DEFAULT, ...raw };
  } catch {
    // nothing kept
  }
  return { ...DEFAULT };
}

/** What each class draws. */
export const x1Marks = $state<Record<MarkClass, string>>(load());

/** Chooses what a class draws, and remembers it. */
export function setMarkAction(c: MarkClass, kind: string) {
  x1Marks[c] = kind;
  ridge = null;
  try {
    localStorage.setItem(KEY, JSON.stringify($state.snapshot(x1Marks)));
  } catch {
    // kept for this visit
  }
}

/** The line being drawn through hits of one class from one station: grows while they keep coming. */
let ridge: { cls: MarkClass; from: string; kind: string; points: [number, number][]; id: string } | null = null;
let seen = 0;

/** A fresh survey session (a new connection or another cave): nothing carries on. */
export function resetMarks(lastSeen = 0) {
  ridge = null;
  seen = lastSeen;
}

const isMark = (m: string | undefined): m is MarkClass => m === 'feature' || m === 'ridge' || m === 'generic';

/** Follows what the survey wrote: each new splay with a class draws what its class is set to. */
export function followLog(log: LogEntry[]) {
  const fresh = log.filter((e) => e.n > seen);
  if (fresh.length === 0) return;
  seen = Math.max(seen, ...fresh.map((e) => e.n));
  const todo = fresh.filter((e) => e.kind === 'splay' && e.shot && isMark(e.reading.mark) && x1Marks[e.reading.mark as MarkClass]);
  // any other shot in between ends a line being grown
  if (fresh.some((e) => !(e.kind === 'splay' && e.reading.mark === 'ridge') && !todo.includes(e))) ridge = null;
  if (todo.length === 0) return;
  const scene = engine.scene(cave.adjusted);
  let drew = false;
  for (const e of todo) {
    const i = scene.splay_shots.indexOf(e.shot!);
    if (i < 0) continue;
    const s = scene.splays[i];
    const from = `${s[0].toFixed(3)},${s[1].toFixed(3)}`;
    const hit: [number, number] = [s[3], s[4]];
    const cls = e.reading.mark as MarkClass;
    const kind = x1Marks[cls];
    if (kind.startsWith('sym-')) {
      // a symbol where the shot hit, facing along the shot
      const dx = s[3] - s[0];
      const dy = s[4] - s[1];
      const len = Math.hypot(dx, dy) || 1;
      if (engine.addMapLine(kind, [hit, [hit[0] + dx / len, hit[1] + dy / len]], null, false, cave.adjusted) === 'ok') drew = true;
      ridge = null;
      continue;
    }
    // a line: the hits of this class from this station, in the order they were shot
    if (ridge && ridge.cls === cls && ridge.from === from && ridge.kind === kind) {
      ridge.points.push(hit);
      if (ridge.id) {
        if (engine.editLine(ridge.id, ridge.points, null, cave.adjusted) === 'ok') drew = true;
      } else if (engine.addMapLine(kind, ridge.points, null, false, cave.adjusted) === 'ok') {
        ridge.id = engine.lastAddedId();
        drew = true;
      }
    } else {
      ridge = { cls, from, kind, points: [hit], id: '' }; // a line needs a second hit
    }
  }
  if (drew) changed();
}
