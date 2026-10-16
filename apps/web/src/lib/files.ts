// Opening and saving a cave on the person's own machine. Nothing is uploaded: the file is read and
// written in the browser. A cave is saved in CaveCAD's own file (.cavecad: the whole cave, with the
// scans it uses bundled in); a DXF is an export, and so is a whole project (everything in the
// cave's folder), and so is the survey alone in one of the standard cave data formats (Compass, Walls,
// Survex, Therion, CSV), which Open and Import also read. Where the File System Access API exists (Chromium) the open file is remembered so
// Save writes back to it; elsewhere Open uses a file input and Save downloads a copy.
import { engine } from './engine';
import { cave, changed, markSaved, markUnsaved, option, unsaved } from './cave.svelte';
import { leave } from './sync.svelte';
import { shelf as shelfState, storeOpenCave } from './shelf.svelte';
import { EXT, pack, unpack } from './cavefile';
import { unzip } from './unzip';
import { zip } from './zip';
import { addRemoteFile, fileOf, scans } from './scans.svelte';
import { readOnly } from './readonly';

const LOCKED = 'Saving and exporting are turned off while you are watching a class: the cave stays with the teacher.';

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
/** The survey data files Open and Import read (Compass, Walls, Survex, Therion, CSV). */
const SURVEY_EXTS = ['.dat', '.srv', '.svx', '.th', '.csv', '.txt', '.mak', '.wpj', '.thconfig'];
/** A project file: one that names the survey files of a cave (Compass .mak, Walls .wpj, Therion thconfig). */
const isProjectFile = (name: string) => /\.(mak|wpj|thconfig)$|(^|[\\/])thconfig(\.[^\\/]*)?$/i.test(name);
const isSurveyFile = (name: string) => SURVEY_EXTS.some((e) => name.toLowerCase().endsWith(e)) || isProjectFile(name);
/** Picked with survey data, never read on their own: Therion sketch pages, drawn onto the map of the survey that pulls them in. */
const SKETCH_EXTS = ['.th2'];
const isSketchFile = (name: string) => SKETCH_EXTS.some((e) => name.toLowerCase().endsWith(e));
const SURVEY_TYPES = [{ description: 'Survey data (Compass, Walls, Survex, Therion, CSV) and Therion sketches', accept: { 'text/plain': [...SURVEY_EXTS, ...SKETCH_EXTS] } }];
const OPEN_TYPES = [{ description: 'A CaveCAD cave, a DXF drawing, a survey data file, or a survey imported from an old drawing', accept: { 'application/zip': [EXT], 'application/dxf': ['.dxf'], 'application/json': ['.json'], 'text/plain': SURVEY_EXTS } }];
const SAVE_TYPES = [{ description: 'CaveCAD cave', accept: { 'application/zip': [EXT] } }];
const DXF_SAVE_TYPES = [{ description: 'DXF drawing', accept: { 'application/dxf': ['.dxf'] } }];

let handle: Handle | null = null;

const isAbort = (e: unknown) => e instanceof DOMException && e.name === 'AbortError';
const safeName = (n: string) => n.replace(/[\\/:*?"<>|]+/g, ' ').trim() || 'cave';
const stem = (n: string) => n.replace(/\.(cavecad|dxf|json|zip|dat|srv|svx|th|csv|txt)$/i, '');

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
    input.accept = `${EXT},.dxf,.json,${SURVEY_EXTS.join(',')},application/zip,application/dxf,image/vnd.dxf,application/json`;
    input.onchange = async () => {
      const file = input.files?.[0];
      resolve(file ? { bytes: await read(file), name: file.name, handle: null } : null);
    };
    input.oncancel = () => resolve(null);
    input.click();
  });
}

/** Picks one or more survey data files (several when a Therion project is split across files). */
async function chooseSurveyFiles(): Promise<{ bytes: Uint8Array; name: string }[] | null> {
  const read = async (f: File) => ({ bytes: new Uint8Array(await f.arrayBuffer()), name: f.name });
  if (picker.showOpenFilePicker) {
    try {
      const hs = await picker.showOpenFilePicker({ types: SURVEY_TYPES, multiple: true });
      return Promise.all(hs.map(async (h) => read(await h.getFile())));
    } catch (e) {
      if (isAbort(e)) return null;
      throw e;
    }
  }
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.multiple = true;
    input.accept = [...SURVEY_EXTS, ...SKETCH_EXTS].join(',');
    input.onchange = async () => resolve(input.files?.length ? await Promise.all([...input.files].map(read)) : null);
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
  if (readOnly.noKeep) throw new Error(LOCKED);
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
  } else if (isSurveyFile(lower)) {
    const r = engine.importSurvey(picked.name, text());
    ok = r.ok;
    message = r.message;
    warnings = r.warnings;
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
  if (readOnly.noKeep) return LOCKED;
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
  if (readOnly.noKeep) return LOCKED;
  const text = engine.saveDxf(cave.adjusted);
  if (!text) return 'Nothing to export: no cave is open.';
  const name = await writeNew(`${stem(cave.fileName)}.dxf`, text, 'application/dxf', DXF_SAVE_TYPES);
  return name ? `Exported ${name}` : null;
}

/**
 * Imports a survey data file (Compass, Walls, Survex, Therion or CSV) as the cave, replacing the one on
 * screen. Several files can be picked at once: a Therion project split across files is read through
 * the one whose `input` lines pull the others in. Returns what to tell the person, or `null` if cancelled.
 */
export async function importSurvey(): Promise<{ ok: boolean; message: string } | null> {
  const picked = await chooseSurveyFiles();
  if (!picked?.length) return null;
  if (!confirmDiscard()) return null;
  const files = picked.map((f) => ({ name: f.name, text: new TextDecoder().decode(f.bytes) }));
  // a project file, or a Therion file that pulls the others in, is the one to read; otherwise the first
  const data = files.filter((f) => !isSketchFile(f.name));
  if (!data.length) return { ok: false, message: 'Sketch pages (.th2) are drawn onto a survey: pick the Therion file that pulls them in, with them.' };
  const main = data.find((f) => isProjectFile(f.name)) ?? data.find((f) => /^\s*input\s/im.test(f.text)) ?? data[0];
  const others = Object.fromEntries(files.filter((f) => f !== main).map((f) => [f.name, f.text]));
  const r = engine.importSurvey(main.name, main.text, others);
  if (!r.ok) return { ok: false, message: r.message };
  leave(); // a different cave: stop sharing the old one
  handle = null;
  cave.fileName = `${stem(main.name)}${EXT}`;
  cave.hasCave = true;
  cave.name = stem(main.name);
  shelf0();
  for (const w of r.warnings) engine.log('warn', 'import', w);
  if (data.length > 1 && !isProjectFile(main.name) && !/^\s*input\s/im.test(main.text)) engine.log('warn', 'import', `Only ${main.name} was read: the other files picked with it were not used.`);
  cave.opened += 1; // the canvas re-reads and re-frames
  changed();
  markUnsaved(); // nothing is a CaveCAD file yet
  void storeOpenCave(main.name);
  return { ok: true, message: `Imported ${main.name}: ${r.message}` };
}

/** The text files of a folder that could be survey data, by path inside it (largest file 20 MB). */
async function chooseFolder(): Promise<{ name: string; files: Record<string, string> } | null> {
  const wanted = (n: string) => isSurveyFile(n) || isSketchFile(n);
  const files: Record<string, string> = {};
  type Dir = { name: string; kind: 'directory'; values(): AsyncIterable<Dir | { name: string; kind: 'file'; getFile(): Promise<File> }> };
  const pickDir = (window as unknown as { showDirectoryPicker?: () => Promise<Dir> }).showDirectoryPicker;
  if (pickDir) {
    let root: Dir;
    try {
      root = await pickDir();
    } catch (e) {
      if (isAbort(e)) return null;
      throw e;
    }
    const walk = async (dir: Dir, at: string, depth: number) => {
      if (depth > 8) return;
      for await (const entry of dir.values()) {
        const path = at ? `${at}/${entry.name}` : entry.name;
        if (entry.kind === 'directory') await walk(entry, path, depth + 1);
        else if (wanted(entry.name)) {
          const f = await entry.getFile();
          if (f.size < 20e6) files[path] = await f.text();
        }
      }
    };
    await walk(root, '', 0);
    return { name: root.name, files };
  }
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    (input as HTMLInputElement & { webkitdirectory: boolean }).webkitdirectory = true;
    input.onchange = async () => {
      const list = [...(input.files ?? [])];
      if (!list.length) return resolve(null);
      for (const f of list) {
        const path = (f as File & { webkitRelativePath?: string }).webkitRelativePath || f.name;
        if (wanted(f.name) && f.size < 20e6) files[path.slice(path.indexOf('/') + 1)] = await f.text();
      }
      const first = (list[0] as File & { webkitRelativePath?: string }).webkitRelativePath ?? '';
      resolve({ name: first.split('/')[0] || 'folder', files });
    };
    input.oncancel = () => resolve(null);
    input.click();
  });
}

/**
 * Imports a whole survey folder as the cave: through its project file (Compass .mak, Walls .wpj,
 * Therion thconfig) when it has one, else every survey file in it as one cave.
 */
export async function importSurveyFolder(): Promise<{ ok: boolean; message: string } | null> {
  const picked = await chooseFolder();
  if (!picked) return null;
  if (!Object.keys(picked.files).length) return { ok: false, message: `${picked.name} holds no survey data files.` };
  if (!confirmDiscard()) return null;
  const r = engine.importFolder(picked.files);
  if (!r.ok) return { ok: false, message: r.message };
  leave(); // a different cave: stop sharing the old one
  handle = null;
  cave.fileName = `${safeName(picked.name)}${EXT}`;
  cave.hasCave = true;
  cave.name = picked.name;
  shelf0();
  for (const w of r.warnings) engine.log('warn', 'import', w);
  cave.opened += 1;
  changed();
  markUnsaved();
  void storeOpenCave(`folder:${picked.name}`);
  return { ok: true, message: `Imported ${picked.name}: ${r.message}` };
}

/** Exports the survey in one of the standard cave data formats (an id from `engine.surveyFormats()`). */
export async function exportSurvey(format: string): Promise<string | null> {
  if (readOnly.noKeep) return LOCKED;
  if (!cave.hasCave) return 'Nothing to export: no cave is open.';
  const r = engine.exportSurvey(format);
  if (!r.ok) return r.message;
  const label = engine.surveyFormats().find((f) => f.id === format)?.label ?? format;
  const name = await writeNew(`${safeName(stem(cave.fileName))}.${r.extension}`, r.text, 'text/plain', [{ description: label, accept: { 'text/plain': [`.${r.extension}`] } }]);
  // a station name the format cannot hold was written another way: say which
  if (name && r.message) engine.log('warn', 'export', r.message);
  return name ? `Exported ${name} (${label})${r.message ? ` -- ${r.message}` : ''}` : null;
}

/** Exports the whole project: the cave, its DXF and everything in the scans folder, as one zip. */
export async function exportProject(): Promise<string | null> {
  if (readOnly.noKeep) return LOCKED;
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
