// What the Cave Shelf does: open a kept cave, open a sample, start a new trip on a cave. Each ends
// with the cave on screen and the shelf closed.
import { engine } from './engine';
import { cave, changed, filter, markSaved } from './cave.svelte';
import { DEMOS } from './demos';
import { confirmDiscard, forgetFile } from './files';
import { dxfOf, flush, isNative, shelf, storeOpenCave } from './shelf.svelte';
import { EXT } from './cavefile';
import { leave } from './sync.svelte';
import { workLayout } from './workspace.svelte';

/** A different cave is now on screen: everything that belonged to the last one is let go. */
function landed(name: string) {
  leave(); // stop sharing the old cave
  forgetFile();
  cave.fileName = `${name}${EXT}`;
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
  const text = await dxfOf(id);
  if (!text) return 'That cave is not on this shelf any more.';
  if (isNative(text)) {
    const r = engine.syncLoad(text);
    if (r !== 'ok') return r.replace(/^error: /, '');
  } else {
    const r = engine.openDxf(text); // a cave kept by an older version
    if (!r.ok) return r.message;
  }
  const rec = shelf.records.find((x) => x.id === id);
  shelf.currentId = id;
  landed(rec?.name ?? engine.shelfSummary()?.name ?? 'Cave');
  markSaved();
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
