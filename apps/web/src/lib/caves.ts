// What the Cave Shelf does: open a kept cave, open a sample, start a new trip on a cave. Each ends
// with the cave on screen and the shelf closed.
import { engine } from './engine';
import { cave, changed, filter, markSaved } from './cave.svelte';
import { DEMOS } from './demos';
import { confirmDiscard, forgetFile } from './files';
import { dxfOf, flush, homeOf, isNative, shelf, storeOpenCave } from './shelf.svelte';
import { folder, openScans, readHome, reachable, setHome, type Home } from './cavefolder.svelte';
import { EXT, unpack } from './cavefile';
import { addRemoteFile, have } from './scans.svelte';
import { typeOf } from './desktop';
import { workLayout } from './workspace.svelte';
import { noRoomForThisCave, serverAccess } from './room.svelte';
import { downloadCave } from './room-client';
import { syncedHash } from './device-key';
import { closeContextMenu } from './context-menu.svelte';
import { describeEntry } from './history-summary';

/** A different cave is now on screen: everything that belonged to the last one is let go. */
function landed(name: string, home: Home | null = null) {
  forgetFile();
  setHome(home);
  shelf.sample = '';
  closeContextMenu(); // a menu opened on the last cave (or the Caves window) has nothing left to act on
  cave.fileName = home?.file ?? `${name}${EXT}`;
  cave.name = name;
  cave.hasCave = true;
  cave.trip = '';
  cave.run = '';
  cave.station = '';
  cave.selected = '';
  filter.hiddenTrips = [];
  cave.opened += 1; // the views re-read and re-frame
  changed();
  shelf.open = false;
}

/**
 * Puts the open cave away: nothing is on screen, nothing can write it back to the shelf, and its room (the live link
 * to the server) closes, as when the app starts with no cave. What was unsaved is kept on the shelf first, so a delete
 * that then fails loses nothing. The engine is given a blank scratch cave so the closed one is not left in memory;
 * it is never stored (autosave needs a cave on screen) and is not counted as unsaved.
 */
export async function closeOpenCave() {
  await flush();
  engine.newCave('Untitled', 'First trip', 'm');
  markSaved();
  forgetFile();
  setHome(null);
  shelf.sample = '';
  shelf.currentId = '';
  closeContextMenu();
  cave.hasCave = false;
  cave.fileName = '';
  cave.name = '';
  cave.trip = '';
  cave.run = '';
  cave.station = '';
  cave.selected = '';
  filter.hiddenTrips = [];
  cave.opened += 1; // the views re-read; the room closes
  changed();
}

/** Opens a cave kept on the shelf. Returns a sentence when it could not. */
export async function openKept(id: string): Promise<string | null> {
  await flush(); // what is open is kept before it is replaced
  if (!confirmDiscard()) return null;
  const rec = shelf.records.find((x) => x.id === id);
  const home = rec ? await homeOf(rec) : null;
  if (home && (await reachable(home, true))) {
    // the file in the cave's folder is the cave; the shelf copy is only the fallback
    const got = await openFileAt(home);
    if (typeof got !== 'string') {
      shelf.currentId = id;
      landed(rec?.name ?? engine.shelfSummary()?.name ?? 'Cave', home);
      markSaved();
      void storeOpenCave(); // the shelf copy catches up with the file
      // the Scans panel shows the folder's scans/, plus any page that is only inside the file
      void openScans(home).then(() => got.forEach((sc) => have(sc.path) || addRemoteFile(sc.path, sc.file)));
      return null;
    }
    engine.log('warn', 'file', `${got} Opened the copy this browser keeps instead.`);
  } else if (rec?.home) {
    engine.log('warn', 'file', `This cave's folder was not found (${rec.home.label}). Opened the copy this browser keeps; nothing is written to the folder until it is found again.`);
  }
  const text = await dxfOf(id);
  if (!text) return 'That cave is not on this shelf any more.';
  if (isNative(text)) {
    const r = engine.syncLoad(text);
    if (r !== 'ok') return r.replace(/^error: /, '');
  } else {
    const r = engine.openDxf(text); // a cave kept by an older version
    if (!r.ok) return r.message;
  }
  shelf.currentId = id;
  landed(rec?.name ?? engine.shelfSummary()?.name ?? 'Cave');
  if (rec?.home) folder.problem = `This cave's folder was not found (${rec.home.label}), so changes are kept in this browser only. Find it from the Cave Shelf.`;
  markSaved();
  return null;
}

/** Loads the cave file from its folder, and gives back the scan pages inside it. A sentence when it could not. */
async function openFileAt(home: Home): Promise<string | { path: string; file: File }[]> {
  let bundle;
  try {
    bundle = await unpack(await readHome(home));
  } catch (e) {
    return `${home.file} could not be read from ${home.label}: ${e instanceof Error ? e.message : String(e)}.`;
  }
  if (!bundle) return `${home.file} in ${home.label} is not a CaveCAD file.`;
  if ('error' in bundle) return bundle.error;
  const r = engine.syncLoad(bundle.cave);
  if (r !== 'ok') return r.replace(/^error: /, '');
  return bundle.scans.map((sc) => ({ path: sc.path, file: new File([sc.data as BlobPart], sc.path.slice(sc.path.lastIndexOf('/') + 1), { type: typeOf(sc.path) }) }));
}

const sha256 = async (s: string) =>
  Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(s))), (b) => b.toString(16).padStart(2, '0')).join('');

/**
 * Opens one of the member's caves from its room on the server (one this shelf does not keep yet): fetches the
 * room's snapshot and history, puts the cave on screen and keeps it on this shelf, where its room keeps it in step
 * from then on. `waiting` when this device has no key for it yet; otherwise a sentence when it could not, or null.
 */
export async function openFromServer(roomId: string): Promise<'waiting' | string | null> {
  await flush();
  if (!confirmDiscard()) return null;
  const access = await serverAccess();
  if (!access) return "Can't reach the CaveCAD server. Check you are online and signed in, then try again.";
  let got;
  try {
    got = await downloadCave(access, roomId);
  } catch (e) {
    console.warn('[rooms] download', e);
    return "The cave could not be fetched from the server. Try again in a moment.";
  }
  if (got.status === 'waiting') return 'waiting';
  if (got.status === 'unknown') return 'That cave is not on the server any more.';
  const r = engine.syncLoad(got.snapshot ?? '[]');
  if (r !== 'ok') return r.replace(/^error: /, '');
  for (const ops of got.entries) engine.syncApply(ops);
  if (engine.roomId() !== roomId) engine.setRoomId(roomId); // a room whose first snapshot was never uploaded
  // All of this copy came from the room: its next open must not post it back whole.
  await syncedHash.save(roomId, await sha256(engine.syncSnapshot()));
  shelf.currentId = '';
  landed(engine.shelfSummary()?.name ?? 'Cave');
  if (!(await storeOpenCave())) return shelf.problem || 'The cave opened but this browser would not keep it.';
  return null;
}

/** A cave's history as the server keeps it, each entry with its number and a few words on what it changed. */
export type CaveHistory = { entries: { number: number; what: string }[]; before: number };

/**
 * Reads a cave's history from its room (the newest snapshot, then every entry after it) without opening it. `before` is
 * how many entries the snapshot already holds: the earliest point that can be gone back to. A sentence when it could not.
 */
export async function readHistory(roomId: string): Promise<CaveHistory | string> {
  const access = await serverAccess();
  if (!access) return "Can't reach the CaveCAD server. Check you are online and signed in, then try again.";
  try {
    const got = await downloadCave(access, roomId);
    if (got.status === 'waiting') return 'This device does not have the cave key yet.';
    if (got.status === 'unknown') return 'That cave is not on the server.';
    return { entries: got.entries.map((ops, i) => ({ number: got.numbers[i], what: describeEntry(ops) })), before: got.numbers[0] ? got.numbers[0] - 1 : 0 };
  } catch (e) {
    console.warn('[rooms] history', e);
    return 'The history could not be fetched from the server. Try again in a moment.';
  }
}

/**
 * Opens an EARLIER version of a server cave as a separate copy: the room's snapshot with only its first `keep` later
 * entries applied. The copy gets a room id of its own, so nothing it does ever reaches the cave it came from (that
 * cave, and its history, are not touched). A sentence when it could not, else null.
 */
export async function openEarlierCopy(roomId: string, keep: number): Promise<string | null> {
  await flush();
  if (!confirmDiscard()) return 'Cancelled.';
  const access = await serverAccess();
  if (!access) return "Can't reach the CaveCAD server. Check you are online and signed in, then try again.";
  let got;
  try {
    got = await downloadCave(access, roomId);
  } catch (e) {
    console.warn('[rooms] earlier version', e);
    return 'The cave could not be fetched from the server. Try again in a moment.';
  }
  if (got.status !== 'ok') return 'That cave could not be fetched from the server.';
  const r = engine.syncLoad(got.snapshot ?? '[]');
  if (r !== 'ok') return r.replace(/^error: /, '');
  for (const ops of got.entries.slice(0, Math.max(0, keep))) engine.syncApply(ops);
  // its own room: a copy that kept the old room id would sync its older state over the cave it was taken from
  const own = 'cave_' + Array.from(crypto.getRandomValues(new Uint8Array(12)), (b) => b.toString(16).padStart(2, '0')).join('');
  const set = engine.setRoomId(own);
  if (set !== 'ok') return set.replace(/^error: /, '');
  shelf.currentId = '';
  const name = engine.shelfSummary()?.name ?? 'Cave';
  landed(`${name} (earlier version)`);
  if (!(await storeOpenCave())) return shelf.problem || 'The copy opened but this browser would not keep it.';
  return null;
}

/** Opens one of the sample caves. Nothing is kept until it is changed: the first change makes it the person's own copy on the shelf. */
export async function openSample(id: string): Promise<string | null> {
  await flush();
  if (!confirmDiscard()) return null;
  const d = DEMOS.find((x) => x.id === id);
  if (!d) return 'No such sample.';
  const r = engine.openDemo(d.json);
  if (r.startsWith('error')) return r.replace(/^error: /, '');
  shelf.currentId = '';
  landed(d.title.replace(/ \(.*\)$/, ''));
  shelf.sample = id;
  noRoomForThisCave(); // a sample never gets a room
  markSaved();
  return null;
}

/**
 * Starts a new trip on a sample cave: the sample becomes a working copy on the shelf (it has to be
 * kept to grow), and the new trip picks up at `station`. Returns a sentence when it could not.
 */
export async function newTripOnSample(id: string, station: string): Promise<string | null> {
  const problem = await openSample(id);
  if (problem) return problem;
  // always a new copy: an earlier copy of this sample is the person's own work and is never written over
  if (!(await storeOpenCave())) return shelf.problem || 'The sample could not be kept.';
  return startTrip(station);
}

/** Starts a trip on the cave now open (tied in at `station`), with the scans and notebook laid out. */
/** Starts a new trip on the open cave picking up at `station` (the right-click menu on a station). A sentence when it could not. */
export function startTripAt(station: string): string | null {
  return startTrip(station);
}

function startTrip(station: string): string | null {
  const trips = engine.notebookTrips();
  const made = engine.notebookNewTrip(`Trip ${trips.length + 1}`);
  if (made.startsWith('error')) return made.replace(/^error: /, '');
  cave.tripFrom = station; // before the trip is shown, so the Notebook finds it
  cave.trip = made;
  changed();
  workLayout(); // the scans and the notebook side by side, ready to type
  void storeOpenCave();
  return null;
}

/**
 * Opens a cave and starts a new trip on it, tied in at `station` when one was picked (the Notebook
 * starts that trip's first shot there). Returns a sentence when it could not.
 */
export async function newTripOn(id: string, station: string): Promise<string | null> {
  const problem = await openKept(id);
  if (problem) return problem;
  return startTrip(station);
}

/** Opens a cave and goes straight to one trip's page. */
export async function openAtTrip(id: string, tripIndex: number): Promise<string | null> {
  const problem = await openKept(id);
  if (problem) return problem;
  const t = engine.notebookTrips()[tripIndex];
  if (t) cave.trip = t.id;
  workLayout();
  return null;
}
