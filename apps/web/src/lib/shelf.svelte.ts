// The Cave Shelf's store: the caves this browser keeps, each with the last summary of what is in it
// (so the shelf can list caves without reading them). A cave is kept as its DXF text, the same file
// Save writes, in IndexedDB on this machine; an open cave is saved back by itself a moment after every
// edit, so closing the page or refreshing loses nothing. Nothing is uploaded.
import { cave, changed, markSaved, unsaved } from './cave.svelte';
import { readOnly } from './readonly';
import { engine, type CaveSummary } from './engine';
import { currentHome, folder, type Home } from './cavefolder.svelte';
import type { Dir } from './folder-handle';

export interface ShelfRecord {
  id: string;
  name: string;
  /** The file name it was imported from, so opening the same file again updates it instead of adding another. */
  source: string;
  favorite: boolean;
  /** Milliseconds since 1970 of the last save. */
  updated: number;
  summary: CaveSummary | null;
  /** Where its project folder is (the browser's handle to it is kept apart, under the same id), if it has one. */
  home?: { label: string; file: string; path?: string };
}

const DB = 'cavecad-shelf';
const STARTUP_KEY = 'cavecad.shelf.startup';

function startupSetting(): boolean {
  try {
    return localStorage.getItem(STARTUP_KEY) !== '0';
  } catch {
    return true;
  }
}

export const shelf = $state({
  /** The shelf window is showing. */
  open: false,
  /** Open it on the Join a live session page (from the ribbon). */
  join: false,
  records: [] as ShelfRecord[],
  /** The record the open cave is saved to ('' for a sample or a cave not yet stored). */
  currentId: '',
  /** Open the shelf when the app starts. */
  startup: startupSetting(),
  /** Why the last save failed, or ''. */
  problem: '',
  /** When the open cave was last stored (ms), or 0. */
  storedAt: 0,
});

export function setStartup(on: boolean) {
  shelf.startup = on;
  try {
    localStorage.setItem(STARTUP_KEY, on ? '1' : '0');
  } catch {
    /* not remembered */
  }
}

function db(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const r = indexedDB.open(DB, 2);
    r.onupgradeneeded = () => {
      const names = r.result.objectStoreNames;
      if (!names.contains('meta')) r.result.createObjectStore('meta', { keyPath: 'id' });
      if (!names.contains('dxf')) r.result.createObjectStore('dxf');
      if (!names.contains('homes')) r.result.createObjectStore('homes'); // a browser's folder handles, by record id
    };
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error);
  });
}

const done = (tx: IDBTransaction) => new Promise<void>((resolve, reject) => {
  tx.oncomplete = () => resolve();
  tx.onerror = () => reject(tx.error);
  tx.onabort = () => reject(tx.error);
});

/** Reads the list of kept caves. */
export async function loadShelf() {
  try {
    const d = await db();
    const all = await new Promise<ShelfRecord[]>((resolve, reject) => {
      const r = d.transaction('meta').objectStore('meta').getAll();
      r.onsuccess = () => resolve(r.result as ShelfRecord[]);
      r.onerror = () => reject(r.error);
    });
    shelf.records = sorted(all);
  } catch {
    shelf.records = [];
  }
}

/** Favourites first, then the most recently used. */
export function sorted(list: ShelfRecord[]): ShelfRecord[] {
  return [...list].sort((a, b) => Number(b.favorite) - Number(a.favorite) || b.updated - a.updated || a.name.localeCompare(b.name));
}

async function put(rec: ShelfRecord, dxf: string | null) {
  const d = await db();
  const tx = d.transaction(['meta', 'dxf'], 'readwrite');
  tx.objectStore('meta').put(JSON.parse(JSON.stringify(rec)));
  if (dxf !== null) tx.objectStore('dxf').put(dxf, rec.id);
  await done(tx);
}

/** The DXF text of a kept cave. */
/** True for a cave kept in CaveCAD's own format (JSON ops); older records are DXF text. */
export const isNative = (text: string): boolean => text.trimStart().startsWith('[');

export async function dxfOf(id: string): Promise<string | null> {
  try {
    const d = await db();
    return await new Promise((resolve) => {
      const r = d.transaction('dxf').objectStore('dxf').get(id);
      r.onsuccess = () => resolve((r.result as string | undefined) ?? null);
      r.onerror = () => resolve(null);
    });
  } catch {
    return null;
  }
}

const newId = () => (typeof crypto !== 'undefined' && 'randomUUID' in crypto ? crypto.randomUUID() : `c${Date.now()}${Math.random().toString(36).slice(2)}`);

/**
 * Stores the open cave on the shelf (a new record when it has none yet). Returns false (with
 * `shelf.problem` saying why) when the browser would not keep it.
 */
export async function storeOpenCave(source = ''): Promise<boolean> {
  if (readOnly.on) return false; // what a student is shown is the teacher's cave: never kept here
  const summary = engine.shelfSummary();
  if (!summary) return false;
  const existing = shelf.records.find((r) => r.id === shelf.currentId) ?? (source ? shelf.records.find((r) => r.source === source) : undefined);
  const rec: ShelfRecord = {
    id: existing?.id ?? newId(),
    name: summary.name,
    source: existing?.source || source,
    favorite: existing?.favorite ?? false,
    updated: Date.now(),
    summary,
  };
  const home = currentHome();
  if (home) rec.home = { label: home.label, file: home.file, ...(home.kind === 'path' ? { path: home.path } : {}) };
  else if (existing?.home) rec.home = existing.home; // a folder not found today is still the cave's folder
  try {
    await put(rec, engine.saveNative()); // the cave in its own format: nothing lost on the way in or out
    if (home?.kind === 'handle') await putHandle(rec.id, home.handle);
  } catch {
    shelf.problem = 'This browser would not keep the cave (storage is full or blocked). Save a file to be safe.';
    return false;
  }
  if (home && homeWriter) {
    // the cave file in its folder is the cave: it is written every time the shelf copy is
    try {
      await homeWriter(home);
      folder.problem = '';
    } catch (e) {
      const problem = `The cave could not be written to its folder (${home.label}): ${e instanceof Error ? e.message : String(e)}`;
      if (problem !== folder.problem) engine.log('error', 'file', problem); // said once, not after every edit
      folder.problem = problem;
    }
  }
  shelf.problem = '';
  shelf.currentId = rec.id;
  shelf.storedAt = rec.updated;
  shelf.records = sorted([...shelf.records.filter((r) => r.id !== rec.id), rec]);
  markSaved();
  return true;
}

let homeWriter: ((home: Home) => Promise<void>) | null = null;

/** How the open cave's file is written into its folder (set by `files.ts`, which packs the file). */
export function setHomeWriter(write: (home: Home) => Promise<void>) {
  homeWriter = write;
}

async function putHandle(id: string, handle: Dir) {
  const d = await db();
  const tx = d.transaction('homes', 'readwrite');
  tx.objectStore('homes').put(handle, id);
  await done(tx);
}

/** A kept cave's folder, as a place to read and write, or `null` (none, or this browser lost its handle). */
export async function homeOf(rec: ShelfRecord): Promise<Home | null> {
  const h = rec.home;
  if (!h) return null;
  if (h.path) return { kind: 'path', path: h.path, file: h.file, label: h.label };
  try {
    const d = await db();
    const handle = await new Promise<Dir | null>((resolve) => {
      const r = d.transaction('homes').objectStore('homes').get(rec.id);
      r.onsuccess = () => resolve((r.result as Dir | undefined) ?? null);
      r.onerror = () => resolve(null);
    });
    return handle ? { kind: 'handle', handle, file: h.file, label: h.label } : null;
  } catch {
    return null;
  }
}

/** Gives a kept cave a (new) folder, or takes its folder away (`null`: kept in this browser only). */
export async function setRecordHome(id: string, home: Home | null) {
  const rec = shelf.records.find((r) => r.id === id);
  if (!rec) return;
  const next: ShelfRecord = { ...rec };
  if (home) next.home = { label: home.label, file: home.file, ...(home.kind === 'path' ? { path: home.path } : {}) };
  else delete next.home;
  await put(next, null);
  const d = await db();
  const tx = d.transaction('homes', 'readwrite');
  if (home?.kind === 'handle') tx.objectStore('homes').put(home.handle, id);
  else tx.objectStore('homes').delete(id);
  await done(tx);
  shelf.records = sorted(shelf.records.map((r) => (r.id === id ? next : r)));
}

/** Removes a cave from the shelf (only the copy kept in this browser). */
export async function forgetCave(id: string) {
  try {
    const d = await db();
    const tx = d.transaction(['meta', 'dxf', 'homes'], 'readwrite');
    tx.objectStore('meta').delete(id);
    tx.objectStore('dxf').delete(id);
    tx.objectStore('homes').delete(id); // the folder on disk is untouched
    await done(tx);
  } catch {
    /* nothing to forget */
  }
  if (shelf.currentId === id) shelf.currentId = '';
  shelf.records = shelf.records.filter((r) => r.id !== id);
}

/** Stars or unstars a cave. */
export async function setFavorite(id: string, on: boolean) {
  const rec = shelf.records.find((r) => r.id === id);
  if (!rec) return;
  const next = { ...rec, favorite: on };
  try {
    await put(next, null);
  } catch {
    return;
  }
  shelf.records = sorted(shelf.records.map((r) => (r.id === id ? next : r)));
}

/** Keeps a summary for a record that has none (read once from its file, then remembered). */
export async function summaryOf(rec: ShelfRecord): Promise<CaveSummary | null> {
  if (rec.summary) return rec.summary;
  const text = await dxfOf(rec.id);
  const summary = text ? (isNative(text) ? engine.shelfSummarizeNative(text) : engine.shelfSummarize(text)) : null;
  if (summary) {
    const next = { ...rec, summary };
    try {
      await put(next, null);
    } catch {
      /* shown but not remembered */
    }
    shelf.records = sorted(shelf.records.map((r) => (r.id === rec.id ? next : r)));
  }
  return summary;
}

let timer: ReturnType<typeof setTimeout> | undefined;

/** After an edit, stores the open cave once things have been quiet for a moment (call when `cave.version` changes). */
export function autosaveSoon() {
  if (!cave.hasCave || !shelf.currentId || !unsaved()) return;
  clearTimeout(timer);
  timer = setTimeout(() => void storeOpenCave(), 1200);
}

/** Stores the open cave now (before the page closes or another cave opens). */
export async function flush() {
  clearTimeout(timer);
  if (cave.hasCave && shelf.currentId && unsaved()) await storeOpenCave();
}

// Dev-only handle for tests and debugging in the browser console.
if (import.meta.env.DEV) (window as unknown as { __shelf: unknown }).__shelf = { shelf, loadShelf, storeOpenCave, changed };
