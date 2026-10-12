// The person's scanned pages. They stay on this machine: a folder is picked (or files dropped), read
// in the browser and shown from a local object URL; nothing is uploaded and nothing but the paths goes
// into the cave (a trip's list of scans, see `cave-scans`). Where the browser can remember a folder
// (Chromium), it is offered again next time with one click (the browser insists on a click to
// let a page read it again).
import { scanKind, type ScanKind } from './scan-tree';

interface DirHandle {
  name: string;
  values(): AsyncIterable<{ kind: 'file' | 'directory'; name: string; getFile?: () => Promise<File> } & Record<string, unknown>>;
  queryPermission?(o: { mode: 'read' }): Promise<string>;
  requestPermission?(o: { mode: 'read' }): Promise<string>;
}
type Picker = { showDirectoryPicker?: (o?: unknown) => Promise<DirHandle> };

/** What the Scans panel shows. Paths are relative to the scans folder. */
export const scans = $state({
  /** Name of the folder (or "dropped files"), for the toolbar. */
  folder: '',
  /** Every scan found, as paths. */
  paths: [] as string[],
  /** The scan on show, or empty. */
  selected: '',
  /** Object URL of the scan on show. */
  url: '',
  kind: 'image' as ScanKind,
  /** A folder remembered from last time that needs a click to be read again. */
  remembered: '',
  /** Folders (paths) the tree has folded. */
  collapsed: [] as string[],
  message: '',
});

const files = new Map<string, File>();
/** Object URLs of scans the map draws, kept so they are made once. */
const mapUrls = new Map<string, string>();
const DB = 'cavecad-scans';

function db(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const r = indexedDB.open(DB, 1);
    r.onupgradeneeded = () => r.result.createObjectStore('folder');
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error);
  });
}

async function keep(handle: DirHandle | null) {
  try {
    const d = await db();
    await new Promise<void>((resolve, reject) => {
      const tx = d.transaction('folder', 'readwrite');
      if (handle) tx.objectStore('folder').put(handle, 'last');
      else tx.objectStore('folder').delete('last');
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
  } catch {
    /* not remembered: private window or no support */
  }
}

async function recall(): Promise<DirHandle | null> {
  try {
    const d = await db();
    return await new Promise((resolve) => {
      const r = d.transaction('folder').objectStore('folder').get('last');
      r.onsuccess = () => resolve((r.result as DirHandle | undefined) ?? null);
      r.onerror = () => resolve(null);
    });
  } catch {
    return null;
  }
}

let remembered: DirHandle | null = null;

/** Looks for a folder remembered from last time (call once when the panel opens). */
export async function checkRemembered() {
  remembered = await recall();
  scans.remembered = remembered?.name ?? '';
}

async function walk(dir: DirHandle, prefix: string, out: Map<string, File>, budget: { n: number }) {
  for await (const entry of dir.values()) {
    if (budget.n <= 0) return;
    if (entry.kind === 'directory') {
      if (entry.name.startsWith('.')) continue;
      await walk(entry as unknown as DirHandle, `${prefix}${entry.name}/`, out, budget);
    } else if (scanKind(`${prefix}${entry.name}`)) {
      budget.n -= 1;
      out.set(`${prefix}${entry.name}`, await entry.getFile!());
    }
  }
}

function load(name: string, found: Map<string, File>) {
  forgetSelection();
  for (const u of mapUrls.values()) URL.revokeObjectURL(u);
  mapUrls.clear();
  files.clear();
  for (const [p, f] of found) files.set(p, f);
  scans.folder = name;
  scans.paths = [...files.keys()];
  scans.collapsed = [];
  scans.message = scans.paths.length ? '' : 'No scans (pictures or PDFs) were found there.';
}

/** Picks a scans folder and reads its picture and PDF files. Returns false if the person cancelled. */
export async function pickFolder(): Promise<boolean> {
  const p = window as unknown as Picker;
  if (!p.showDirectoryPicker) {
    scans.message = 'This browser cannot open a folder. Drop the scan files onto the panel instead.';
    return false;
  }
  try {
    const handle = await p.showDirectoryPicker({ mode: 'read' });
    await readFolder(handle);
    await keep(handle);
    remembered = handle;
    scans.remembered = '';
    return true;
  } catch (e) {
    if (e instanceof DOMException && e.name === 'AbortError') return false;
    scans.message = 'The folder could not be read.';
    return false;
  }
}

async function readFolder(handle: DirHandle) {
  const found = new Map<string, File>();
  await walk(handle, '', found, { n: 5000 });
  load(handle.name, found);
}

/** Opens the folder remembered from last time (needs a click: the browser asks again before reading). */
export async function reopenRemembered(): Promise<boolean> {
  if (!remembered) return false;
  try {
    const ok = (await remembered.queryPermission?.({ mode: 'read' })) === 'granted' || (await remembered.requestPermission?.({ mode: 'read' })) === 'granted';
    if (!ok) return false;
    await readFolder(remembered);
    scans.remembered = '';
    return true;
  } catch {
    scans.message = 'The remembered folder could not be read.';
    return false;
  }
}

/**
 * Adds files dropped on the panel, or picked one by one (a browser that cannot open a folder). A
 * file that arrived inside a folder keeps its folder path.
 */
export function addFiles(list: File[]) {
  const found = new Map<string, File>(files);
  for (const f of list) {
    const rel = (f as File & { webkitRelativePath?: string }).webkitRelativePath || f.name;
    if (scanKind(rel)) found.set(rel, f);
  }
  load(scans.folder || 'dropped files', found);
}

/** Puts a scan on show. */
export function select(path: string) {
  const f = files.get(path);
  if (!f) return;
  forgetSelection();
  scans.selected = path;
  scans.kind = scanKind(path) ?? 'image';
  scans.url = URL.createObjectURL(f);
}

function forgetSelection() {
  if (scans.url) URL.revokeObjectURL(scans.url);
  scans.url = '';
  scans.selected = '';
}

/** A URL the map can load a scan from (made once per scan), or `null` when the scan is not in the open folder. */
export function mapUrl(path: string): string | null {
  const f = files.get(path);
  if (!f) return null;
  let u = mapUrls.get(path);
  if (!u) {
    u = URL.createObjectURL(f);
    mapUrls.set(path, u);
  }
  return u;
}

/** Aligning: a spot on the scan waiting for the station it is. */
export const align = $state({ on: false, pending: null as { px: number; py: number } | null });

/** Trimming: the outline being traced, point by point. */
export const trimming = $state({ on: false, draft: [] as [number, number][] });

/** True when the scan is in the folder now open. */
export const have = (path: string) => files.has(path);

/** Folds or unfolds a tree folder. */
export function toggleFolder(path: string) {
  scans.collapsed = scans.collapsed.includes(path) ? scans.collapsed.filter((p) => p !== path) : [...scans.collapsed, path];
}

// Dev-only handle for tests and debugging in the browser console.
if (import.meta.env.DEV) (window as unknown as { __scans: unknown }).__scans = { addFiles, select, scans };
