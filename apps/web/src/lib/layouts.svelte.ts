// Saved layouts for the person using the app: six slots, kept on this device. A layout is where the panels are
// (and whether the map view is locked); the workspace says how to read the arrangement from the dock and how to
// put one back by registering a host, so the app's workspace and any other dock (the survey harness) share this.
// The slot rules are in layout-slots.ts, which is tested.

import { option, setOption } from './cave.svelte';
import { cleanName, emptySlots, fitsPanels, parseSlots, slotName, withSlot, withoutSlot, type Slots } from './layout-slots';

const KEY = 'cavecad.layouts.v1';

function load(): Slots {
  try {
    return parseSlots(JSON.parse(localStorage.getItem(KEY) ?? 'null'));
  } catch {
    return emptySlots();
  }
}

export const layouts = $state<{ slots: Slots; selected: number }>({ slots: load(), selected: 0 });

function keep() {
  try {
    localStorage.setItem(KEY, JSON.stringify($state.snapshot(layouts.slots)));
  } catch {
    /* the layouts are just not remembered */
  }
}

/** What a dock must offer: its arrangement as data, and a way to bring one back (false when it cannot). */
export interface LayoutHost {
  capture(): unknown;
  apply(layout: unknown): boolean;
  /** True for a panel id this dock has. */
  has(id: string): boolean;
}

let host: LayoutHost | null = null;

/** Says which dock the layouts belong to (`null` when it goes away). */
export function setLayoutHost(h: LayoutHost | null) {
  host = h;
}

/** Keeps the arrangement on screen now in slot `i` under `name`. Returns a sentence when it could not. */
export function saveSlot(i: number, name: string): string {
  const layout = host?.capture();
  if (!layout) return 'There is no workspace to save.';
  layouts.slots = withSlot(layouts.slots, i, { name: cleanName(name, i), layout, mapLock: option('mapLock', false) });
  layouts.selected = i;
  keep();
  return '';
}

/** Brings slot `i` back. Returns a sentence when it could not (an empty slot, or one from a version with other panels). */
export function recallSlot(i: number): string {
  const s = layouts.slots[i];
  if (!s) return `${slotName(layouts.slots, i)} is empty.`;
  if (!host || !fitsPanels(s.layout, host.has)) return `${s.name} uses panels this version does not have.`;
  if (!host.apply(s.layout)) return `${s.name} could not be restored.`;
  setOption('mapLock', s.mapLock);
  layouts.selected = i;
  return '';
}

/** Empties slot `i`. */
export function clearSlot(i: number) {
  layouts.slots = withoutSlot(layouts.slots, i);
  keep();
}
