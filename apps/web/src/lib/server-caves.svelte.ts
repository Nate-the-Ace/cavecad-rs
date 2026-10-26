// The member's caves kept on the CaveCAD server that this browser's shelf does not have yet: caves made or kept
// on another device (rooms step 1 keeps every own cave's locked copy there). Listed by their labels, which are
// locked with each cave's key, so a cave shows its name only once this device has been handed that key; opened by
// fetching the room's snapshot and history (caves.ts `openFromServer`).
//
// A device with no key for a cave waits: another of the member's devices hands keys on when it signs in, every few
// minutes while open, whenever it opens that cave (room.svelte.ts), and whenever its own shelf lists these. While
// the shelf is open the list is read again every 15 s, and a cave someone asked to open opens by itself once its
// key has arrived.
import { handOverKeys, listMyCaves } from './room-client';
import { serverAccess } from './room.svelte';
import { shelfRoomIds } from './shelf.svelte';
import type { CaveSummary } from './engine';

export type OtherCave = {
  id: string;
  /** When its room was made (ms). */
  createdAt: number;
  /** This device has the cave's key: it can be opened now. */
  hasKey: boolean;
  /** The name from its label, or null when the label is not readable here yet. */
  name: string | null;
  summary: CaveSummary | null;
};

export const serverCaves = $state({
  list: [] as OtherCave[],
  /** Signed in and the server answered: the list is what the server holds. */
  loaded: false,
  /** Why the list could not be read ('' when it could). */
  problem: '',
  /** A cave being fetched now. */
  opening: '',
  /** A cave someone asked to open that is waiting for its key: it opens by itself once the key is here. */
  waitingFor: '',
});

/** Reads a label: `{name, summary?}` as JSON text, or null. */
function readLabel(text: string | null): { name: string | null; summary: CaveSummary | null } {
  if (!text) return { name: null, summary: null };
  try {
    const v = JSON.parse(text) as { name?: unknown; summary?: unknown };
    return { name: typeof v.name === 'string' && v.name ? v.name : null, summary: v.summary && typeof v.summary === 'object' ? (v.summary as CaveSummary) : null };
  } catch {
    return { name: null, summary: null };
  }
}

let reading: Promise<void> | null = null;

/** Reads the member's caves on the server again, keeping those this shelf does not have. Needs the engine loaded. */
export function refreshServerCaves(): Promise<void> {
  reading ??= (async () => {
    try {
      const access = await serverAccess();
      if (!access) {
        serverCaves.list = [];
        serverCaves.loaded = false;
        return;
      }
      void handOverKeys(access).catch(() => 0); // this device may hold keys another one is waiting for
      const all = await listMyCaves(access);
      const here = await shelfRoomIds();
      serverCaves.list = all
        .filter((c) => !here.has(c.id))
        .map((c) => ({ id: c.id, createdAt: c.createdAt, hasKey: c.hasKey, ...readLabel(c.label) }))
        .reverse(); // newest first
      serverCaves.loaded = true;
      serverCaves.problem = '';
      if (serverCaves.waitingFor && !serverCaves.list.some((c) => c.id === serverCaves.waitingFor)) serverCaves.waitingFor = '';
    } catch {
      serverCaves.problem = "Can't reach the CaveCAD server to list caves on your other devices.";
    } finally {
      reading = null;
    }
  })();
  return reading;
}
