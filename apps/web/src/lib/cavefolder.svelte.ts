// The open cave's project folder (spec: docs/specs/2026-10-26-new-cave-folder.md). New cave asks
// where the folder goes, makes it with its standard subfolders and writes the `.cavecad` into it;
// after that every save writes there too. The `.cavecad` file is the cave: the folder is where it
// lives, with the originals and outputs around it.
//
// Two ways to reach a folder: the desktop app by its real path, a Chromium browser by a directory
// handle. Safari and Firefox have neither (`canMakeFolders()` is false), and keep the shelf only.
import { engine } from './engine';
import { EXT } from './cavefile';
import {
  documentsFolderNative,
  folderExistsNative,
  isDesktop,
  lookCavePlaceNative,
  makeCaveFolderNative,
  pickCavePlaceNative,
  readCaveNative,
  revealNative,
  writeCaveNative,
  writeExportNative,
} from './desktop';
import { allowed, exportsIn, isCaveFolder, lookIn, makeIn, readIn, scansIn, writeIn, type Dir, type Plan } from './folder-handle';
import { openHandle, openNative } from './scans.svelte';

/** Where a cave's folder is. */
export type Home = { kind: 'path'; path: string; file: string; label: string } | { kind: 'handle'; handle: Dir; file: string; label: string };

/** A place picked for a new cave's folder (the folder goes inside it). */
export type Place = { kind: 'path'; path: string; label: string } | { kind: 'handle'; handle: Dir; label: string };

type Picker = { showDirectoryPicker?: (o?: unknown) => Promise<Dir> };

const plan: Plan = (inside, backups, stem, now) => engine.folderPlan(inside, backups, stem, now);
const LAST_PLACE = 'cavecad.newcave.place';

/** What the app shows about the open cave's folder. */
export const folder = $state({
  /** Where it is, for showing (`''`: the cave has no folder). */
  label: '',
  /** Why the last write to it failed, or ''. */
  problem: '',
});

let current: Home | null = null;

/** The open cave's folder, or `null`. */
export const currentHome = (): Home | null => current;

/** The open cave's folder is now `home` (or none). */
export function setHome(home: Home | null) {
  current = home;
  folder.label = home?.label ?? '';
  folder.problem = '';
}

/** True where a cave can have a folder: the desktop app, or a Chromium browser outside a frame. */
export function canMakeFolders(): boolean {
  if (isDesktop()) return true;
  return typeof (window as unknown as Picker).showDirectoryPicker === 'function' && window.self === window.top;
}

const isAbort = (e: unknown) => e instanceof DOMException && e.name === 'AbortError';

/** Where the New cave dialog starts before anything is picked: the desktop app's last place, else Documents. */
export async function startPlace(): Promise<Place | null> {
  if (!isDesktop()) return null; // a browser only names a place once it is picked
  let last = '';
  try {
    last = localStorage.getItem(LAST_PLACE) ?? '';
  } catch {
    /* not remembered */
  }
  if (last && (await folderExistsNative(last))) return { kind: 'path', path: last, label: last };
  const docs = await documentsFolderNative();
  return docs ? { kind: 'path', path: docs, label: docs } : null;
}

/** Asks where the folder goes, starting at Documents (or the last place chosen). `null` if cancelled. */
export async function choosePlace(from: Place | null): Promise<Place | null> {
  if (isDesktop()) {
    const path = await pickCavePlaceNative(from?.kind === 'path' ? from.path : null);
    if (!path) return null;
    try {
      localStorage.setItem(LAST_PLACE, path);
    } catch {
      /* not remembered */
    }
    return { kind: 'path', path, label: path };
  }
  const p = window as unknown as Picker;
  try {
    // `id` makes the browser start next time where this one ended; the first time it starts in Documents
    const handle = await p.showDirectoryPicker!({ id: 'cave-place', startIn: 'documents', mode: 'readwrite' });
    return { kind: 'handle', handle, label: handle.name };
  } catch (e) {
    if (isAbort(e)) return null;
    throw e;
  }
}

/** The cave's folder name for `name`. */
export const folderNameOf = (name: string): string => engine.folderName(name);

const join = (a: string, b: string) => `${a.replace(/[\\/]+$/, '')}${a.includes('\\') && !a.includes('/') ? '\\' : '/'}${b}`;

/** What is already where the cave's folder would go, and how to show that place. */
export async function look(place: Place, name: string): Promise<{ label: string; place: ReturnType<Plan>['place'] }> {
  const dir = folderNameOf(name);
  if (place.kind === 'path') {
    const r = await lookCavePlaceNative(place.path, name);
    return { label: r.folder, place: r.place };
  }
  const label = isCaveFolder(place.handle, dir) ? place.label : `${place.label}/${dir}`;
  return { label, place: await lookIn(place.handle, dir, plan) };
}

/** Makes (or completes) the cave's folder in `place`, for a cave file called `file`. */
export async function make(place: Place, name: string, file = `${folderNameOf(name)}${EXT}`): Promise<Home> {
  const dir = folderNameOf(name);
  if (place.kind === 'path') {
    const path = await makeCaveFolderNative(place.path, name);
    return { kind: 'path', path, file, label: path };
  }
  const handle = await makeIn(place.handle, dir, plan);
  return { kind: 'handle', handle, file, label: isCaveFolder(place.handle, dir) ? place.label : `${place.label}/${dir}` };
}

/** The person's local time to the minute, as backups are named. */
export function stampOf(d = new Date()): string {
  const two = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())} ${two(d.getHours())}${two(d.getMinutes())}`;
}

/** Writes the cave file into its folder (a dated backup of the last copy is taken first). */
export async function writeHome(home: Home, bytes: Uint8Array): Promise<void> {
  if (home.kind === 'path') return writeCaveNative(home.path, home.file, bytes, stampOf());
  return writeIn(home.handle, home.file, bytes, stampOf(), plan);
}

/** The cave file's bytes from its folder. */
export async function readHome(home: Home): Promise<Uint8Array> {
  if (home.kind === 'path') return new Uint8Array(await readCaveNative(home.path, home.file));
  return readIn(home.handle, home.file);
}

/** True when the folder can be written now. `ask` lets a browser ask for permission (needs a click). */
export async function reachable(home: Home, ask: boolean): Promise<boolean> {
  try {
    if (home.kind === 'path') return await folderExistsNative(home.path);
    if (!(await allowed(home.handle, ask))) return false;
    for await (const _ of home.handle.values()) break; // a folder that was deleted throws here
    return true;
  } catch {
    return false;
  }
}

/** Opens the Scans panel on the cave's `scans/`. */
export async function openScans(home: Home): Promise<boolean> {
  if (home.kind === 'path') return openNative(join(home.path, engine.folderPlan([], [], '', '').scans));
  return openHandle((await scansIn(home.handle, plan)) as unknown as Parameters<typeof openHandle>[0]);
}

/**
 * Desktop: writes an export straight into the cave's `exports/` and says where. Browser: the folder
 * the save dialog should start in (the person can still choose somewhere else).
 */
export async function exportPlace(home: Home, name: string, bytes: Uint8Array): Promise<{ wrote: string } | { startIn: Dir }> {
  if (home.kind === 'path') return { wrote: await writeExportNative(home.path, name, bytes) };
  return { startIn: await exportsIn(home.handle, plan) };
}

/** Opens Finder or Explorer on the folder (desktop app only). */
export async function revealHome(home: Home): Promise<boolean> {
  if (home.kind !== 'path') return false;
  await revealNative(home.path, '');
  return true;
}

/** Asks for an existing cave folder (to find one that moved). It must hold `file`. */
export async function findFolder(file: string): Promise<Home | string | null> {
  if (isDesktop()) {
    const path = await pickCavePlaceNative(null);
    if (!path) return null;
    const home: Home = { kind: 'path', path, file, label: path };
    try {
      await readHome(home);
      return home;
    } catch {
      return `${file} is not in that folder.`;
    }
  }
  try {
    const handle = await (window as unknown as Picker).showDirectoryPicker!({ id: 'cave-place', mode: 'readwrite' });
    const home: Home = { kind: 'handle', handle, file, label: handle.name };
    try {
      await readIn(handle, file);
      return home;
    } catch {
      return `${file} is not in that folder.`;
    }
  } catch (e) {
    if (isAbort(e)) return null;
    throw e;
  }
}
