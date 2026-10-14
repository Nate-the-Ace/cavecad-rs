// Saved layouts: a fixed number of slots, each holding an arrangement of the workspace's panels (what dockview
// writes with `toJSON`) and the view setting that goes with it. Pure and tested (tests/layoutslots.check.ts);
// what is kept on the device and what talks to the dock is `layouts.svelte.ts`.

/** How many layouts can be kept. */
export const LAYOUT_SLOTS = 6;

/** One kept layout. */
export interface SavedLayout {
  name: string;
  /** The dock's own description of where every panel is (dockview's `toJSON`). */
  layout: unknown;
  /** Whether the map view was locked when it was saved: a layout for sketching in the cave brings the lock with it. */
  mapLock: boolean;
}

export type Slots = (SavedLayout | null)[];

export const emptySlots = (): Slots => Array.from({ length: LAYOUT_SLOTS }, () => null);

const isObject = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);

/** Reads what was kept, trusting nothing: a slot that is not well formed is an empty one, and there are always exactly `LAYOUT_SLOTS`. */
export function parseSlots(raw: unknown): Slots {
  const out = emptySlots();
  if (!Array.isArray(raw)) return out;
  for (let i = 0; i < LAYOUT_SLOTS; i++) {
    const s = raw[i];
    if (isObject(s) && typeof s.name === 'string' && isObject(s.layout) && typeof s.mapLock === 'boolean') out[i] = { name: s.name, layout: s.layout, mapLock: s.mapLock };
  }
  return out;
}

const inRange = (i: number) => Number.isInteger(i) && i >= 0 && i < LAYOUT_SLOTS;

/** The slots with `saved` in slot `i` (unchanged for a slot that does not exist). */
export function withSlot(slots: Slots, i: number, saved: SavedLayout): Slots {
  return inRange(i) ? slots.map((s, k) => (k === i ? saved : s)) : slots;
}

/** The slots with slot `i` emptied. */
export function withoutSlot(slots: Slots, i: number): Slots {
  return inRange(i) ? slots.map((s, k) => (k === i ? null : s)) : slots;
}

/** What a slot is called: its own name, or "Layout 3" for slot 2 (people count from one). */
export const slotName = (slots: Slots, i: number): string => slots[i]?.name ?? `Layout ${i + 1}`;

/** A name as typed, made fit to keep: trimmed and at most 30 characters; empty means the default for that slot. */
export function cleanName(text: string, i: number): string {
  const t = text.replace(/\s+/g, ' ').trim().slice(0, 30);
  return t || `Layout ${i + 1}`;
}

/** The ids of the panels a dock layout holds. */
export function panelIds(layout: unknown): string[] {
  return isObject(layout) && isObject(layout.panels) ? Object.keys(layout.panels) : [];
}

/** True when a layout can be brought back here: it holds at least one panel and every one is a panel this workspace has. */
export function fitsPanels(layout: unknown, has: (id: string) => boolean): boolean {
  const ids = panelIds(layout);
  return ids.length > 0 && ids.every(has);
}
