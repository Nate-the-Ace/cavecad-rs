// Saved layouts for the person using the app: six slots, kept on this device and, when signed in, with their CaveCAD
// account, so they come back on any device they sign in on (the newer copy wins). A layout is where the panels are
// (and whether the map view is locked); the workspace says how to read the arrangement from the dock and how to
// put one back by registering a host, so the app's workspace and any other dock (the survey harness) share this.
// The slot rules are in layout-slots.ts, which is tested.

import { option, setOption } from './cave.svelte';
import { cleanName, emptySlots, fitsPanels, parseSlots, slotName, withSlot, withoutSlot, type Slots } from './layout-slots';

const KEY = 'cavecad.layouts.v1';
/** When the slots on this device were last changed (or taken from the account). */
const AT_KEY = 'cavecad.layouts.at';

function loadAt(): number {
  try {
    return Number(localStorage.getItem(AT_KEY) ?? 0) || 0;
  } catch {
    return 0;
  }
}
let changedAt = loadAt();

function load(): Slots {
  try {
    return parseSlots(JSON.parse(localStorage.getItem(KEY) ?? 'null'));
  } catch {
    return emptySlots();
  }
}

export const layouts = $state<{ slots: Slots; selected: number }>({ slots: load(), selected: 0 });

function store(at: number) {
  changedAt = at;
  try {
    localStorage.setItem(KEY, JSON.stringify($state.snapshot(layouts.slots)));
    localStorage.setItem(AT_KEY, String(at));
  } catch {
    /* the layouts are just not remembered */
  }
}

function keep() {
  store(Date.now());
  scheduleSend();
}

// ---- with the account ----
let sendTimer: ReturnType<typeof setTimeout> | null = null;
function scheduleSend() {
  if (sendTimer) clearTimeout(sendTimer);
  sendTimer = setTimeout(() => void send(), 800);
}

async function access() {
  const { serverAccess } = await import('./room.svelte'); // loaded when needed: the workspace loads this file early
  return serverAccess();
}

/** Sends this device's slots to the account (the server keeps whichever copy is newer). */
async function send(): Promise<void> {
  const a = await access();
  if (!a) return;
  try {
    await a.fetch(`${a.base}/api/me/prefs/layouts`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json', ...(a.headers?.() ?? {}) },
      body: JSON.stringify({ value: $state.snapshot(layouts.slots), updatedAt: changedAt }),
      credentials: 'same-origin',
    });
  } catch {
    /* offline: sent again at the next change or sign-in */
  }
}

/**
 * Brings this device and the account into step (call on sign-in): the account's slots replace these when they are newer;
 * these are sent when they are newer (or the account has none).
 */
export async function syncLayouts(): Promise<void> {
  const a = await access();
  if (!a) return;
  try {
    const res = await a.fetch(`${a.base}/api/me/prefs/layouts`, { headers: a.headers?.() ?? {}, credentials: 'same-origin' });
    if (!res.ok) return;
    const { pref } = (await res.json()) as { pref: { value: unknown; updatedAt: number } | null };
    if (pref && pref.updatedAt > changedAt) {
      layouts.slots = parseSlots(pref.value);
      store(pref.updatedAt);
    } else if (changedAt > (pref?.updatedAt ?? -1) && layouts.slots.some(Boolean)) {
      await send();
    }
  } catch {
    /* offline: the slots on this device stay as they are */
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
