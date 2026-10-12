// The Cave Shelf's store: the caves this browser keeps, each with the last summary of what is in it
// (so the shelf can list caves without reading them). A cave is kept as its DXF text, the same file
// Save writes, in IndexedDB on this machine; an open cave is saved back by itself a moment after every
// edit, so closing the page or refreshing loses nothing. Nothing is uploaded.
import { cave, changed, markSaved, unsaved } from './cave.svelte';
import { engine, type CaveSummary } from './engine';

export interface ShelfRecord {
  id: string;
  name: string;
  /** The file name it was imported from, so opening the same file again updates it instead of adding another. */
  source: string;
  favorite: boolean;
  /** Milliseconds since 1970 of the last save. */
  updated: number;
  summary: CaveSummary | null;
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
    const r = indexedDB.open(DB, 1);
    r.onupgradeneeded = () => {
      r.result.createObjectStore('meta', { keyPath: 'id' });
      r.result.createObjectStore('dxf');
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
  try {
    await put(rec, engine.saveDxf(cave.adjusted));
  } catch {
    shelf.problem = 'This browser would not keep the cave (storage is full or blocked). Save a file to be safe.';
    return false;
  }
  shelf.problem = '';
  shelf.currentId = rec.id;
  shelf.storedAt = rec.updated;
  shelf.records = sorted([...shelf.records.filter((r) => r.id !== rec.id), rec]);
  markSaved();
  return true;
}

/** Removes a cave from the shelf (only the copy kept in this browser). */
export async function forgetCave(id: string) {
  try {
    const d = await db();
    const tx = d.transaction(['meta', 'dxf'], 'readwrite');
    tx.objectStore('meta').delete(id);
    tx.objectStore('dxf').delete(id);
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
  const summary = text ? engine.shelfSummarize(text) : null;
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
