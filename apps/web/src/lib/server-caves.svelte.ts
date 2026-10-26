// The member's caves kept on the CaveCAD server, for the Cave Shelf ("On your other devices": those this shelf does
// not keep yet) and the Rooms panel (all of them, and the member's computers). Caves are listed by their labels: an
// open cave's label is plain text, so any of the member's devices lists it by name and opens it at once, by fetching
// the room's snapshot and history (caves.ts `openFromServer`); no key is involved.
//
// Caves locked during rooms step 1 (until a device holding their key opens them once, which converts them to open)
// show their name only once this device has been handed the key. Such a device waits: another of the member's
// devices hands keys on when it signs in, every few minutes while open, whenever it opens that cave (room.svelte.ts),
// whenever its shelf or Rooms panel reads this list, and when someone presses Send keys now. While the shelf or the
// Rooms panel is open the list is read again every 15 s, and a cave someone asked to open opens by itself once its
// key has arrived or it has been converted.
import { handOverKeys, listMyCaves, type ServerAccess } from './room-client';
import { serverAccess } from './room.svelte';
import { shelf, shelfRoomIds } from './shelf.svelte';
import { openFromServer, openKept } from './caves';
import { fingerprint } from './room-crypto';
import type { CaveSummary } from './engine';

export type OtherCave = {
  id: string;
  /** When its room was made (ms). */
  createdAt: number;
  /** It can be opened here now: an open cave always, a locked one when this device has its key. */
  hasKey: boolean;
  /** The name from its label, or null when the label is not readable here yet. */
  name: string | null;
  summary: CaveSummary | null;
  /** The shelf record that keeps it on this computer, or '' when this shelf does not have it. */
  shelfId: string;
};

/** One of the member's computers (browsers count as computers: each keeps its own key). */
export type Computer = { id: string; code: string; createdAt: number; lastSeen: number | null; here: boolean };

export const serverCaves = $state({
  /** Every cave of the member's on the server, newest first. */
  all: [] as OtherCave[],
  /** Those this shelf does not keep yet (the Cave Shelf's "On your other devices"). */
  list: [] as OtherCave[],
  computers: [] as Computer[],
  /** Signed in and the server answered: the lists are what the server holds. */
  loaded: false,
  /** Why the lists could not be read ('' when they could). */
  problem: '',
  /** A cave being fetched now. */
  opening: '',
  /** A locked cave someone asked to open that is waiting for its key: it opens by itself once it can. */
  waitingFor: '',
  /** What the last Send keys now did. */
  sent: '',
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

async function readComputers(access: ServerAccess): Promise<Computer[]> {
  const res = await access.fetch(access.base + '/api/devices', { headers: { ...(access.headers?.() ?? {}) }, credentials: 'same-origin' });
  if (!res.ok) throw new Error(String(res.status));
  const { devices } = (await res.json()) as { devices: { id: string; publicKey: string; createdAt?: number; lastSeen?: number | null }[] };
  const out: Computer[] = [];
  for (const d of devices) {
    let code = '';
    try {
      code = await fingerprint(d.publicKey);
    } catch {
      code = '?';
    }
    out.push({ id: d.id, code, createdAt: d.createdAt ?? 0, lastSeen: d.lastSeen ?? null, here: d.id === access.device.id });
  }
  return out;
}

let reading: Promise<void> | null = null;

/** Reads the member's caves and computers on the server again. Needs the engine loaded. */
export function refreshServerCaves(): Promise<void> {
  reading ??= (async () => {
    try {
      const access = await serverAccess();
      if (!access) {
        Object.assign(serverCaves, { all: [], list: [], computers: [], loaded: false });
        return;
      }
      void handOverKeys(access).catch(() => 0); // this device may hold keys another one is waiting for
      const caves = await listMyCaves(access);
      await shelfRoomIds(); // fills in the room id of records kept before rooms
      const shelfIdOf = (room: string) => shelf.records.find((r) => r.roomId === room)?.id ?? '';
      serverCaves.all = caves.map((c) => ({ id: c.id, createdAt: c.createdAt, hasKey: c.hasKey, ...readLabel(c.label), shelfId: shelfIdOf(c.id) })).reverse();
      serverCaves.list = serverCaves.all.filter((c) => !c.shelfId);
      serverCaves.computers = await readComputers(access);
      serverCaves.loaded = true;
      serverCaves.problem = '';
      const want = serverCaves.waitingFor;
      if (want && !serverCaves.list.some((c) => c.id === want)) serverCaves.waitingFor = '';
      else if (want && serverCaves.list.some((c) => c.id === want && c.hasKey) && !serverCaves.opening) void openServerCave(want);
    } catch {
      serverCaves.problem = "Can't reach the CaveCAD server.";
    } finally {
      reading = null;
    }
  })();
  return reading;
}

/**
 * Opens one of the member's caves: from this shelf when it keeps it, else fetched from the server. One with no key
 * here yet waits and opens by itself when the key arrives. Returns a sentence when it could not, or ''.
 */
export async function openServerCave(id: string): Promise<string> {
  if (serverCaves.opening) return '';
  const kept = serverCaves.all.find((c) => c.id === id)?.shelfId;
  serverCaves.opening = id;
  try {
    if (kept) return (await openKept(kept)) ?? '';
    const r = await openFromServer(id);
    if (r === 'waiting') {
      serverCaves.waitingFor = id;
      return '';
    }
    if (serverCaves.waitingFor === id) serverCaves.waitingFor = '';
    if (!r) void refreshServerCaves(); // it is on this shelf now
    return r ?? '';
  } finally {
    serverCaves.opening = '';
  }
}

/** Gives the member's other computers the keys this one has, now. */
export async function sendKeysNow(): Promise<void> {
  serverCaves.sent = 'Sending…';
  const access = await serverAccess();
  if (!access) {
    serverCaves.sent = "Can't reach the CaveCAD server, or not signed in.";
    return;
  }
  try {
    const n = await handOverKeys(access);
    serverCaves.sent = n === 0 ? 'Your other computers already have every key this one has.' : `Sent the keys of ${n} cave${n === 1 ? '' : 's'}.`;
  } catch {
    serverCaves.sent = "Can't reach the CaveCAD server.";
  }
  void refreshServerCaves();
}
