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
  images.clear();
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
  // Inside a frame (the hosted copy) the directory picker is refused, and the plain folder input must
  // be opened straight from the click, so it is used from the start.
  const framed = window.self !== window.top;
  if (p.showDirectoryPicker && !framed) {
    try {
      const handle = await p.showDirectoryPicker({ mode: 'read' });
      await readFolder(handle);
      await keep(handle);
      remembered = handle;
      scans.remembered = '';
      return true;
    } catch (e) {
      if (e instanceof DOMException && e.name === 'AbortError') return false;
      // Not allowed here (a page inside a frame, as on the hosted copy): use the plain folder input.
    }
  }
  return pickFolderWithInput();
}

/**
 * The folder picker every browser has: a file input that takes a whole folder. It cannot be
 * remembered for next time, but it works where the directory picker is blocked.
 */
function pickFolderWithInput(): Promise<boolean> {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.multiple = true;
    input.setAttribute('webkitdirectory', '');
    input.onchange = () => {
      const list = [...(input.files ?? [])];
      if (!list.length) return resolve(false);
      const found = new Map<string, File>();
      let folder = '';
      for (const f of list) {
        const rel = (f as File & { webkitRelativePath?: string }).webkitRelativePath || f.name;
        const parts = rel.split('/');
        folder = parts.length > 1 ? parts[0] : folder;
        const inside = parts.length > 1 ? parts.slice(1).join('/') : rel; // paths are relative to the folder itself
        if (scanKind(inside)) found.set(inside, f);
      }
      load(folder || 'scans', found);
      resolve(true);
    };
    input.oncancel = () => resolve(false);
    input.click();
  });
}

/** Reads a dropped folder or file entry (and everything inside a folder) into `out`. */
async function walkEntry(entry: FileSystemEntry, prefix: string, out: Map<string, File>, budget: { n: number }) {
  if (budget.n <= 0) return;
  if (entry.isFile) {
    const path = `${prefix}${entry.name}`;
    if (!scanKind(path)) return;
    budget.n -= 1;
    out.set(path, await new Promise<File>((res, rej) => (entry as FileSystemFileEntry).file(res, rej)));
  } else if (entry.isDirectory && !entry.name.startsWith('.')) {
    const reader = (entry as FileSystemDirectoryEntry).createReader();
    for (;;) {
      const batch = await new Promise<FileSystemEntry[]>((res, rej) => reader.readEntries(res, rej));
      if (!batch.length) break;
      for (const child of batch) await walkEntry(child, `${prefix}${entry.name}/`, out, budget);
    }
  }
}

/**
 * Adds what was dropped on the panel: files, or a whole folder (its scans, at any depth, keeping
 * their folder paths). A dropped folder becomes the scans folder.
 */
export async function dropItems(items: DataTransferItemList): Promise<void> {
  const entries = [...items].map((i) => i.webkitGetAsEntry?.()).filter((e): e is FileSystemEntry => !!e);
  const folders = entries.filter((e) => e.isDirectory);
  const found = new Map<string, File>();
  if (folders.length === 1 && entries.length === 1) {
    // One folder dropped: its own name is the scans folder, so paths start inside it.
    const root = folders[0] as FileSystemDirectoryEntry;
    const reader = root.createReader();
    for (;;) {
      const batch = await new Promise<FileSystemEntry[]>((res, rej) => reader.readEntries(res, rej));
      if (!batch.length) break;
      for (const child of batch) await walkEntry(child, '', found, { n: 5000 });
    }
    load(root.name, found);
    return;
  }
  for (const e of entries) await walkEntry(e, '', found, { n: 5000 });
  for (const [path, f] of files) if (!found.has(path)) found.set(path, f);
  load(scans.folder || 'dropped files', found);
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

/**
 * Aligning: a spot on the scan waiting for what it is. `view` says which drawing the scan is being
 * placed in (`map`, `profile:<run>` or `section:<station>`); `point` is a place clicked in that
 * drawing, which the Scans panel picks up as the spot's match.
 */
export const align = $state({
  on: false,
  pending: null as { px: number; py: number } | null,
  /** Which drawing the scan is being placed in: the Map, the Profile's run on show, or the Cross Section's station on show. */
  kind: 'map' as 'map' | 'profile' | 'section',
  view: 'map',
  point: { x: 0, y: 0, seq: 0 },
});

/** Whether a drawing is the one a scan is being aligned in and is waiting for a clicked place. */
export const waitingForPoint = (view: string): boolean => align.on && align.pending !== null && align.view === view;

/** Hands a place clicked in a drawing to the Scans panel. */
export function givePoint(x: number, y: number) {
  align.point = { x, y, seq: align.point.seq + 1 };
}

const images = new Map<string, HTMLImageElement>();

/**
 * The picture of a scan for a 2D view to draw (made once per scan), or `null` while it loads or when
 * the scan is not in the open folder. `redraw` is called when it has loaded.
 */
export function scanImage(path: string, redraw: () => void): HTMLImageElement | null {
  let img = images.get(path);
  if (!img) {
    const url = mapUrl(path);
    if (!url) return null;
    img = new Image();
    img.onload = redraw;
    img.src = url;
    images.set(path, img);
  }
  return img.complete && img.naturalWidth > 0 ? img : null;
}

/** Trimming: the outline being traced, point by point. */
export const trimming = $state({ on: false, draft: [] as [number, number][] });

/** True when the scan is in the folder now open. */
export const have = (path: string) => files.has(path);

/** Folds (`closed`) or unfolds several tree folders at once. */
export function setFolders(folders: string[], closed: boolean) {
  const set = new Set(scans.collapsed);
  for (const f of folders) {
    if (closed) set.add(f);
    else set.delete(f);
  }
  scans.collapsed = [...set];
}

/** Folds or unfolds a tree folder. */
export function toggleFolder(path: string) {
  scans.collapsed = scans.collapsed.includes(path) ? scans.collapsed.filter((p) => p !== path) : [...scans.collapsed, path];
}

// Dev-only handle for tests and debugging in the browser console.
if (import.meta.env.DEV) (window as unknown as { __scans: unknown }).__scans = { addFiles, select, scans, align };
