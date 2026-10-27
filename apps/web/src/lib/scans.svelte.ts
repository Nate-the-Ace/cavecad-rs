// The person's scanned pages. They stay on this machine: a folder is picked (or files dropped), read
// in the browser and shown from a local object URL; nothing is uploaded and nothing but the paths goes
// into the cave (a trip's list of scans, see `cave-scans`). Where the browser can remember a folder
// (Chromium), it is offered again next time with one click (the browser insists on a click to
// let a page read it again).
import { baseName, isDesktop, listScansNative, pickFolderNative, readScanNative, revealNative, typeOf } from './desktop';
import { isDerived, scanKind, type ScanKind } from './scan-tree';

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
  /** The pages on show came back from the copy this browser keeps (not read from the folder just now). */
  restored: false,
  /** Bytes of the kept copy, or 0. */
  keptBytes: 0,
  /** The real path of the folder the pages were read from (desktop app only), or ''. */
  root: '',
  /** Pages read so far while a folder is being read from disk, as `n of m`, or ''. */
  loading: '',
});

const files = new Map<string, File>();
/** Object URLs of scans the map draws, kept so they are made once. */
const mapUrls = new Map<string, string>();
const DB = 'cavecad-scans';

function db(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const r = indexedDB.open(DB, 2);
    r.onupgradeneeded = () => {
      const names = r.result.objectStoreNames;
      if (!names.contains('folder')) r.result.createObjectStore('folder');
      if (!names.contains('files')) r.result.createObjectStore('files'); // path to the page itself
    };
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

/**
 * Keeps a copy of the pages in this browser, so a refresh (or coming back tomorrow) finds them
 * again with no folder to pick. Stored on this machine only. If there is no room, the pages work for
 * this visit and the person is told they will be picked again next time.
 */
async function keepCopy(name: string, found: Map<string, File>) {
  try {
    const d = await db();
    await new Promise<void>((resolve, reject) => {
      const tx = d.transaction(['files', 'folder'], 'readwrite');
      const store = tx.objectStore('files');
      store.clear();
      let bytes = 0;
      for (const [path, f] of found) {
        store.put(f, path);
        bytes += f.size;
      }
      tx.objectStore('folder').put({ name, bytes, count: found.size }, 'copy');
      tx.oncomplete = () => {
        scans.keptBytes = bytes;
        resolve();
      };
      tx.onerror = () => reject(tx.error);
      tx.onabort = () => reject(tx.error);
    });
  } catch {
    scans.keptBytes = 0;
    scans.message = 'This browser had no room to keep a copy of the scans, so they will need to be opened again after a refresh.';
  }
}

/** Brings back the pages kept from last time. Returns whether there were any. */
async function restoreCopy(): Promise<boolean> {
  try {
    const d = await db();
    const meta = await new Promise<{ name: string; bytes: number } | undefined>((resolve) => {
      const r = d.transaction('folder').objectStore('folder').get('copy');
      r.onsuccess = () => resolve(r.result);
      r.onerror = () => resolve(undefined);
    });
    if (!meta) return false;
    const found = new Map<string, File>();
    await new Promise<void>((resolve) => {
      const r = d.transaction('files').objectStore('files').openCursor();
      r.onsuccess = () => {
        const c = r.result;
        if (!c) return resolve();
        if (c.value instanceof Blob) found.set(String(c.key), c.value as File);
        c.continue();
      };
      r.onerror = () => resolve();
    });
    if (!found.size) return false;
    load(meta.name, found, false);
    scans.restored = true;
    scans.keptBytes = meta.bytes;
    return true;
  } catch {
    return false;
  }
}

/** Throws away the kept copy (the folder on disk is untouched). */
export async function forgetCopy() {
  try {
    const d = await db();
    await new Promise<void>((resolve) => {
      const tx = d.transaction(['files', 'folder'], 'readwrite');
      tx.objectStore('files').clear();
      tx.objectStore('folder').delete('copy');
      tx.oncomplete = () => resolve();
      tx.onerror = () => resolve();
    });
  } catch {
    /* nothing kept */
  }
  scans.keptBytes = 0;
  scans.restored = false;
}

let remembered: DirHandle | null = null;

/** Looks for a folder remembered from last time (call once when the panel opens). */
let starting: Promise<void> | null = null;

export function checkRemembered(): Promise<void> {
  // Once only, however many parts of the app ask: the app at start-up (so scans are on the maps
  // before the panel is ever opened) and the panel.
  starting ??= (async () => {
    if (isDesktop()) {
      // The desktop app reads the folder from where it really is, fresh every time.
      let last = '';
      try {
        last = localStorage.getItem(LAST_ROOT_KEY) ?? '';
      } catch {
        /* nothing remembered */
      }
      if (last && scans.paths.length === 0 && (await openNative(last))) return;
      scans.message = '';
    }
    if (scans.paths.length === 0) await restoreCopy(); // pages kept from last time come back by themselves
    remembered = await recall();
    scans.remembered = remembered?.name ?? '';
  })();
  return starting;
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

function load(name: string, found: Map<string, File>, saveCopy = true) {
  forgetSelection();
  for (const u of mapUrls.values()) URL.revokeObjectURL(u);
  mapUrls.clear();
  images.clear();
  files.clear();
  for (const [p, f] of found) files.set(p, f);
  scans.folder = name;
  // Trimmed pages stay loaded (the views draw them) but are not the person's own scans: not listed here.
  scans.paths = [...files.keys()].filter((p) => !isDerived(p));
  scans.collapsed = [];
  scans.restored = false;
  scans.message = scans.paths.length ? '' : 'No scans (pictures or PDFs) were found there.';
  if (saveCopy && found.size) void keepCopy(name, found);
}

const LAST_ROOT_KEY = 'cavecad.scans.lastRoot';

/**
 * Desktop app: reads a folder straight from disk by its real path. The path is remembered (so the
 * folder comes back fresh on every start, with nothing to pick, and a changed page is the changed
 * page), and is what Reveal opens. No copy is kept in the browser.
 */
export async function openNative(root: string): Promise<boolean> {
  scans.loading = 'Reading the folder…';
  try {
    const list = (await listScansNative(root)).filter((e) => scanKind(e.path));
    const found = new Map<string, File>();
    let next = 0;
    let done = 0;
    const worker = async () => {
      while (next < list.length) {
        const e = list[next++];
        const bytes = await readScanNative(root, e.path);
        found.set(e.path, new File([bytes], e.path.slice(e.path.lastIndexOf('/') + 1), { type: typeOf(e.path) }));
        done += 1;
        if (done % 10 === 0) scans.loading = `Reading the scans: ${done} of ${list.length}`;
      }
    };
    await Promise.all(Array.from({ length: Math.min(6, list.length) }, worker));
    load(baseName(root), found, false);
    scans.root = root;
    scans.keptBytes = 0;
    setRealRoot(scans.folder, root);
    try {
      localStorage.setItem(LAST_ROOT_KEY, root);
    } catch {
      /* not remembered */
    }
    return true;
  } catch (e) {
    scans.message = `The folder could not be read: ${e instanceof Error ? e.message : String(e)}`;
    return false;
  } finally {
    scans.loading = '';
  }
}

/** Desktop app: opens Finder or Explorer on a page or folder of the scans folder (the real one). */
export async function revealReal(rel: string): Promise<boolean> {
  try {
    await revealNative(scans.root, rel);
    return true;
  } catch (e) {
    scans.message = `Could not show it: ${e instanceof Error ? e.message : String(e)}`;
    return false;
  }
}

/** Picks a scans folder and reads its picture and PDF files. Returns false if the person cancelled. */
export async function pickFolder(): Promise<boolean> {
  if (isDesktop()) {
    const root = await pickFolderNative();
    return root ? openNative(root) : false;
  }
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

/** Reads a folder the app already has a handle for (a cave's own `scans/`), and remembers it for next time. */
export async function openHandle(handle: DirHandle): Promise<boolean> {
  try {
    await readFolder(handle);
    await keep(handle);
    remembered = handle;
    scans.remembered = '';
    return true;
  } catch {
    scans.message = 'The scans folder could not be read.';
    return false;
  }
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
  /** Cross Section: the second pick is a wall straight out from the station, which sets the scale. */
  wall: true,
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

const ROOTS_KEY = 'cavecad.scans.roots';

function roots(): Record<string, string> {
  try {
    const raw = JSON.parse(localStorage.getItem(ROOTS_KEY) ?? 'null');
    if (raw && typeof raw === 'object') return raw;
  } catch {
    /* nothing saved */
  }
  return {};
}

/** Where the folder called `name` really lives on this computer, as the person told us (this browser only), or ''. */
export const realRoot = (name: string): string => roots()[name] ?? '';

/** Remembers where the folder called `name` lives. An empty path forgets it. */
export function setRealRoot(name: string, path: string) {
  const all = roots();
  if (path) all[name] = path;
  else delete all[name];
  try {
    localStorage.setItem(ROOTS_KEY, JSON.stringify(all));
  } catch {
    /* not remembered: private window */
  }
}

/** Lets go of every page held in memory and forgets the folder (a student leaving a class: the teacher's pages must not stay). */
export function clearScans() {
  for (const u of mapUrls.values()) URL.revokeObjectURL(u);
  mapUrls.clear();
  files.clear();
  images.clear();
  scans.paths = [];
  scans.selected = '';
  scans.folder = '';
  scans.message = '';
}

/** A page by its path, for sending to someone else. */
export const fileOf = (path: string): File | undefined => files.get(path);

/** Adds a page that arrived from another device (kept for this visit only: it is not a folder on this machine). */
export function addRemoteFile(path: string, file: File) {
  if (!scanKind(path) || isDerived(path)) return;
  files.set(path, file);
  if (!scans.paths.includes(path)) scans.paths = [...scans.paths, path].sort();
  if (!scans.folder) scans.folder = 'from the host';
}
