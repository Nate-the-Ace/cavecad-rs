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

/** A different cave is now on screen: everything that belonged to the last one is let go. */
function landed(name: string, home: Home | null = null) {
  forgetFile();
  setHome(home);
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

/** Opens one of the sample caves (a working copy; nothing is kept unless it is saved). */
export async function openSample(id: string): Promise<string | null> {
  await flush();
  if (!confirmDiscard()) return null;
  const d = DEMOS.find((x) => x.id === id);
  if (!d) return 'No such sample.';
  const r = engine.openDemo(d.json);
  if (r.startsWith('error')) return r.replace(/^error: /, '');
  shelf.currentId = '';
  landed(d.title.replace(/ \(.*\)$/, ''));
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
  if (!(await storeOpenCave(`sample:${id}`))) return shelf.problem || 'The sample could not be kept.';
  return startTrip(station);
}

/** Starts a trip on the cave now open (tied in at `station`), with the scans and notebook laid out. */
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
