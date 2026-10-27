// A cave's project folder in a Chromium browser, through the File System Access API: making it with
// its subfolders, writing the cave file (backing up the last copy first), reading it back, and
// finding a subfolder. What to make, back up and delete is decided by `cave-folder` (passed in as
// `plan`, so this file runs without the engine in tests); only the reading and writing are here.

import type { FolderPlan } from './engine';

/** The parts of a directory handle used here. */
export interface Dir {
  kind: 'directory';
  name: string;
  values(): AsyncIterable<{ kind: 'file' | 'directory'; name: string }>;
  getDirectoryHandle(name: string, o?: { create?: boolean }): Promise<Dir>;
  getFileHandle(name: string, o?: { create?: boolean }): Promise<FileH>;
  removeEntry(name: string): Promise<void>;
  queryPermission?(o: { mode: 'read' | 'readwrite' }): Promise<string>;
  requestPermission?(o: { mode: 'read' | 'readwrite' }): Promise<string>;
}

/** The parts of a file handle used here. */
export interface FileH {
  kind: 'file';
  name: string;
  getFile(): Promise<Blob>;
  createWritable(): Promise<{ write(data: Uint8Array): Promise<void>; close(): Promise<void> }>;
}

/** The project folder's rules: `engine.folderPlan`. */
export type Plan = (inside: string[] | null, backups: string[], stem: string, now: string) => FolderPlan;

const notFound = (e: unknown) => e instanceof DOMException && (e.name === 'NotFoundError' || e.name === 'TypeMismatchError');

/** The names directly inside a folder. */
export async function namesIn(dir: Dir): Promise<string[]> {
  const out: string[] = [];
  for await (const e of dir.values()) out.push(e.name);
  return out.sort();
}

/** A folder inside `dir`, or `null` when there is none. */
export async function childDir(dir: Dir, name: string): Promise<Dir | null> {
  try {
    return await dir.getDirectoryHandle(name);
  } catch (e) {
    if (notFound(e)) return null;
    throw e;
  }
}

/** What is under `folder` in `parent`: nothing, a folder with a cave in it, or one without. */
export async function lookIn(parent: Dir, folder: string, plan: Plan): Promise<FolderPlan['place']> {
  const dir = await childDir(parent, folder);
  return plan(dir ? await namesIn(dir) : null, [], '', '').place;
}

/** Makes (or completes) the cave's folder under `parent` with the subfolders it is missing. Nothing in it is touched. */
export async function makeIn(parent: Dir, folder: string, plan: Plan): Promise<Dir> {
  const dir = await parent.getDirectoryHandle(folder, { create: true });
  for (const sub of plan(await namesIn(dir), [], '', '').missing) await dir.getDirectoryHandle(sub, { create: true });
  return dir;
}

async function bytesOf(h: FileH): Promise<Uint8Array> {
  return new Uint8Array(await (await h.getFile()).arrayBuffer());
}

async function put(dir: Dir, name: string, bytes: Uint8Array) {
  const w = await (await dir.getFileHandle(name, { create: true })).createWritable();
  await w.write(bytes);
  await w.close(); // the browser writes a hidden copy and swaps it in on close: never half a file
}

/**
 * Writes the cave file into its folder. When an older copy is there it goes into `backup/` first
 * (at most once in ten minutes, `stamp` being local time as `yyyy-mm-dd hhmm`), and old backups are
 * thinned. Only this cave's own backups are ever deleted.
 */
export async function writeIn(folder: Dir, file: string, bytes: Uint8Array, stamp: string, plan: Plan): Promise<void> {
  const stem = file.replace(/\.cavecad$/i, '');
  let old: FileH | null = null;
  try {
    old = await folder.getFileHandle(file);
  } catch (e) {
    if (!notFound(e)) throw e;
  }
  if (old) {
    const top = plan(await namesIn(folder), [], stem, stamp);
    const backup = await folder.getDirectoryHandle(top.backupFolder, { create: true });
    const first = plan(null, await namesIn(backup), stem, stamp);
    if (first.backupDue && first.backupName) await put(backup, first.backupName, await bytesOf(old));
    for (const gone of plan(null, await namesIn(backup), stem, stamp).prune) await backup.removeEntry(gone);
  }
  await put(folder, file, bytes);
}

/** The cave file's bytes. */
export async function readIn(folder: Dir, file: string): Promise<Uint8Array> {
  return bytesOf(await folder.getFileHandle(file));
}

/** The folder exports go to (`exports/`, or an old cave's `PDF/`), made when missing. */
export async function exportsIn(folder: Dir, plan: Plan): Promise<Dir> {
  return folder.getDirectoryHandle(plan(await namesIn(folder), [], '', '').exports, { create: true });
}

/** The scans folder, made when missing. */
export async function scansIn(folder: Dir, plan: Plan): Promise<Dir> {
  return folder.getDirectoryHandle(plan(await namesIn(folder), [], '', '').scans, { create: true });
}

/** True when the browser lets the page write the folder; asks (a click must be behind this) when `ask`. */
export async function allowed(dir: Dir, ask: boolean): Promise<boolean> {
  const mode = { mode: 'readwrite' as const };
  if ((await dir.queryPermission?.(mode)) === 'granted') return true;
  if (!ask) return false;
  return (await dir.requestPermission?.(mode)) === 'granted';
}
