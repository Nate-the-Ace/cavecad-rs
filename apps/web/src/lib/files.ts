// Opening and saving a cave on the person's own machine. Nothing is uploaded: the file is read and
// written in the browser. A cave is saved in CaveCAD's own file (.cavecad: the whole cave, with the
// scans it uses bundled in); a DXF is an export, and so is a whole project (everything in the
// cave's folder). Where the File System Access API exists (Chromium) the open file is remembered so
// Save writes back to it; elsewhere Open uses a file input and Save downloads a copy.
import { engine } from './engine';
import { cave, changed, markSaved, markUnsaved, option, unsaved } from './cave.svelte';
import { leave } from './sync.svelte';
import { shelf as shelfState, storeOpenCave } from './shelf.svelte';
import { EXT, pack, unpack } from './cavefile';
import { unzip } from './unzip';
import { zip } from './zip';
import { addRemoteFile, fileOf, scans } from './scans.svelte';

/** The subset of the File System Access API this file uses. */
interface Handle {
  name: string;
  getFile(): Promise<File>;
  createWritable(): Promise<{ write(data: string | Uint8Array): Promise<void>; close(): Promise<void> }>;
}
type Picker = {
  showOpenFilePicker?: (o: unknown) => Promise<Handle[]>;
  showSaveFilePicker?: (o: unknown) => Promise<Handle>;
};
const picker = window as unknown as Picker;
const OPEN_TYPES = [{ description: 'A CaveCAD cave, a DXF drawing, or a survey imported from an old drawing', accept: { 'application/zip': [EXT], 'application/dxf': ['.dxf'], 'application/json': ['.json'] } }];
const SAVE_TYPES = [{ description: 'CaveCAD cave', accept: { 'application/zip': [EXT] } }];
const DXF_SAVE_TYPES = [{ description: 'DXF drawing', accept: { 'application/dxf': ['.dxf'] } }];

let handle: Handle | null = null;

const isAbort = (e: unknown) => e instanceof DOMException && e.name === 'AbortError';
const safeName = (n: string) => n.replace(/[\\/:*?"<>|]+/g, ' ').trim() || 'cave';
const stem = (n: string) => n.replace(/\.(cavecad|dxf|json|zip)$/i, '');

/** Picks a file: with the API when there is one, otherwise with a throwaway `<input type=file>`. */
async function chooseFile(): Promise<{ bytes: Uint8Array; name: string; handle: Handle | null } | null> {
  const read = async (f: File) => new Uint8Array(await f.arrayBuffer());
  if (picker.showOpenFilePicker) {
    try {
      const [h] = await picker.showOpenFilePicker({ types: OPEN_TYPES, multiple: false });
      return { bytes: await read(await h.getFile()), name: h.name, handle: h };
    } catch (e) {
      if (isAbort(e)) return null;
      throw e;
    }
  }
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = `${EXT},.dxf,.json,application/zip,application/dxf,image/vnd.dxf,application/json`;
    input.onchange = async () => {
      const file = input.files?.[0];
      resolve(file ? { bytes: await read(file), name: file.name, handle: null } : null);
    };
    input.oncancel = () => resolve(null);
    input.click();
  });
}

/** Hands the browser a copy to download. */
function download(name: string, data: Uint8Array | string, type: string) {
  const url = URL.createObjectURL(new Blob([data as BlobPart], { type }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  a.click();
  URL.revokeObjectURL(url);
}

/** Writes a file where the person chooses (or downloads it where there is no picker). Returns the name, or `null` if cancelled. */
async function writeNew(name: string, data: Uint8Array | string, type: string, types: unknown): Promise<string | null> {
  try {
    if (picker.showSaveFilePicker) {
      const h = await picker.showSaveFilePicker({ suggestedName: name, types });
      const w = await h.createWritable();
      await w.write(data);
      await w.close();
      return h.name;
    }
  } catch (e) {
    if (isAbort(e)) return null;
    throw e;
  }
  download(name, data, type);
  return name;
}

/** Asks before a cave on screen with unsaved changes is replaced. True means go ahead. */
export function confirmDiscard(): boolean {
  if (!unsaved()) return true;
  return window.confirm('This cave has changes you have not saved. Replace it anyway?');
}

/** Warns before the page is closed or reloaded with unsaved changes (the browser words the question). */
export function guardUnload() {
  const onUnload = (e: BeforeUnloadEvent) => {
    if (!unsaved()) return;
    e.preventDefault();
    e.returnValue = ''; // some browsers need this to show their prompt
  };
  window.addEventListener('beforeunload', onUnload);
  return () => window.removeEventListener('beforeunload', onUnload);
}

const MIME: Record<string, string> = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', gif: 'image/gif', webp: 'image/webp', bmp: 'image/bmp', tif: 'image/tiff', tiff: 'image/tiff', pdf: 'application/pdf' };
const mimeOf = (path: string) => MIME[path.slice(path.lastIndexOf('.') + 1).toLowerCase()] ?? 'application/octet-stream';

function openSurveyJson(text: string): { ok: boolean; message: string; warnings: string[] } {
  const m = engine.openDemo(text);
  return m.startsWith('error') ? { ok: false, message: m.replace(/^error: /, ''), warnings: [] } : { ok: true, message: m, warnings: [] };
}

/** The scan pages this cave uses (those linked to a trip) that are open here, as bytes. */
async function usedScans(): Promise<{ path: string; data: Uint8Array }[]> {
  const out: { path: string; data: Uint8Array }[] = [];
  for (const path of new Set(engine.scanLinks().map((l) => l.path))) {
    const f = fileOf(path);
    if (f) out.push({ path, data: new Uint8Array(await f.arrayBuffer()) });
  }
  return out;
}

/** The bytes of the open cave as a CaveCAD file, with its scans when asked. */
export async function caveBytes(withScans: boolean): Promise<Uint8Array> {
  return pack({ name: cave.name || stem(cave.fileName), app: `cavecad-rs ${engine.version()}`, cave: engine.syncSnapshot(), scans: withScans ? await usedScans() : [] });
}

/**
 * Starts a brand new cave with one empty trip, replacing the one on screen (after asking, if it has
 * unsaved changes). Returns what to tell the person, or `null` if they declined.
 */
export function startCave(name: string, unit: 'ft' | 'm'): { ok: boolean; message: string } | null {
  if (!confirmDiscard()) return null;
  const clean = name.trim() || 'New cave';
  const r = engine.newCave(clean, 'First trip', unit);
  if (r !== 'ok') return { ok: false, message: r.replace(/^error: /, '') };
  leave(); // a different cave: stop sharing the old one
  handle = null;
  cave.fileName = `${safeName(clean)}${EXT}`;
  cave.trip = '';
  cave.run = '';
  cave.station = '';
  cave.hasCave = true;
  cave.name = clean;
  shelf0();
  cave.opened += 1; // the canvas re-reads and re-frames
  changed();
  markUnsaved();
  void storeOpenCave(); // a new cave goes on the shelf at once, and is kept as it grows
  return { ok: true, message: `Started ${clean}` };
}

/** Asks for a file and opens it. Returns what to tell the person, or `null` if they cancelled. */
export async function openCave(): Promise<{ ok: boolean; message: string } | null> {
  const picked = await chooseFile();
  if (!picked) return null;
  if (!confirmDiscard()) return null;
  const lower = picked.name.toLowerCase();
  const text = () => new TextDecoder().decode(picked.bytes);
  let ok = false;
  let message = '';
  let warnings: string[] = [];
  let native = false;
  let bundled: { path: string; data: Uint8Array }[] = [];
  if (picked.bytes[0] === 0x50 && picked.bytes[1] === 0x4b) {
    // a zip: a CaveCAD file, or a whole exported project holding one
    let bundle = await unpack(picked.bytes);
    if (!bundle) {
      const inner = (await unzip(picked.bytes))?.find((f) => f.name.toLowerCase().endsWith(EXT));
      if (inner) bundle = await unpack(inner.data);
    }
    if (!bundle) return { ok: false, message: `${picked.name} is not a CaveCAD file.` };
    if ('error' in bundle) return { ok: false, message: bundle.error };
    const r = engine.syncLoad(bundle.cave);
    ok = r === 'ok';
    message = ok ? `${bundle.manifest.name}` : r.replace(/^error: /, '');
    native = true;
    bundled = bundle.scans;
  } else if (lower.endsWith('.json')) {
    // A survey written by tools/import_old_dxf.mjs (a one-off importer for old drawings) opens as a demo-style cave.
    const r = openSurveyJson(text());
    ok = r.ok;
    message = r.message;
  } else {
    const r = engine.openDxf(text());
    ok = r.ok;
    message = r.message;
    warnings = r.warnings;
  }
  if (!ok) return { ok: false, message };
  leave(); // a different cave: stop sharing the old one
  handle = native ? picked.handle : null; // only a CaveCAD file is saved back to where it came from
  cave.fileName = native ? picked.name : `${stem(picked.name)}${EXT}`;
  cave.hasCave = true;
  cave.name = stem(picked.name);
  shelf0();
  for (const w of warnings) engine.log('warn', 'file', w);
  // the scans that travelled inside the file are there to use, as if their folder were open
  for (const s of bundled) addRemoteFile(s.path, new File([s.data as BlobPart], s.path.slice(s.path.lastIndexOf('/') + 1), { type: mimeOf(s.path) }));
  if (bundled.length) {
    scans.folder = scans.folder === 'from the host' || !scans.folder ? 'inside the file' : scans.folder;
    engine.log('info', 'file', `${bundled.length} scan page${bundled.length === 1 ? '' : 's'} came inside the file`);
  }
  cave.opened += 1; // the canvas re-reads and re-frames
  changed();
  markSaved();
  void storeOpenCave(picked.name); // a file opened is kept on the shelf (the same file again updates its record)
  return { ok: true, message: `Opened ${picked.name}${native ? '' : ': ' + message}` };
}

/** Saves the open cave as a CaveCAD file. `saveAs` always asks where. Returns what to tell the person, or `null` if they cancelled. */
export async function saveCave(saveAs = false): Promise<string | null> {
  if (!cave.hasCave) return 'Nothing to save: no cave is open.';
  const bytes = await caveBytes(option('saveScans', true));
  try {
    if (picker.showSaveFilePicker) {
      if (!handle || saveAs) handle = await picker.showSaveFilePicker({ suggestedName: cave.fileName, types: SAVE_TYPES });
      const w = await handle.createWritable();
      await w.write(bytes);
      await w.close();
      cave.fileName = handle.name;
      markSaved();
      return `Saved ${handle.name}`;
    }
  } catch (e) {
    if (isAbort(e)) return null;
    throw e;
  }
  download(cave.fileName, bytes, 'application/zip');
  markSaved();
  return `Downloaded ${cave.fileName}`;
}

/** Exports the drawing as a DXF (for other CAD programs). It is not how a cave is saved. */
export async function exportDxf(): Promise<string | null> {
  const text = engine.saveDxf(cave.adjusted);
  if (!text) return 'Nothing to export: no cave is open.';
  const name = await writeNew(`${stem(cave.fileName)}.dxf`, text, 'application/dxf', DXF_SAVE_TYPES);
  return name ? `Exported ${name}` : null;
}

/** Exports the whole project: the cave, its DXF and everything in the scans folder, as one zip. */
export async function exportProject(): Promise<string | null> {
  if (!cave.hasCave) return 'Nothing to export: no cave is open.';
  const base = safeName(stem(cave.fileName));
  const entries = [
    { name: `${base}/${base}${EXT}`, data: await caveBytes(false) },
    { name: `${base}/${base}.dxf`, data: new TextEncoder().encode(engine.saveDxf(cave.adjusted)) },
  ];
  for (const path of scans.paths) {
    const f = fileOf(path);
    if (f) entries.push({ name: `${base}/scans/${path}`, data: new Uint8Array(await f.arrayBuffer()) });
  }
  const name = await writeNew(`${base}-project.zip`, zip(entries), 'application/zip', [{ description: 'Project folder (zip)', accept: { 'application/zip': ['.zip'] } }]);
  return name ? `Exported ${name}: the cave, its DXF and ${entries.length - 2} scan page${entries.length === 3 ? '' : 's'}` : null;
}

/** The next demo or file that opens starts with no remembered file to save back to. */
export function forgetFile() {
  handle = null;
}

/** A different cave is now open: it is not any record of the shelf until it is stored. */
function shelf0() {
  shelfState.currentId = '';
}
