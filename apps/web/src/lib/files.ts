// Opening and saving a cave as a DXF file on the person's own machine. Nothing is uploaded: the
// file is read and written in the browser. Where the File System Access API exists (Chromium) the
// open file is remembered so Save writes back to it; elsewhere Open uses a file input and Save
// downloads a copy.
import { engine } from './engine';
import { cave, changed, markSaved, unsaved } from './cave.svelte';
import { leave } from './sync.svelte';

/** The subset of the File System Access API this file uses. */
interface Handle {
  name: string;
  getFile(): Promise<File>;
  createWritable(): Promise<{ write(data: string): Promise<void>; close(): Promise<void> }>;
}
type Picker = {
  showOpenFilePicker?: (o: unknown) => Promise<Handle[]>;
  showSaveFilePicker?: (o: unknown) => Promise<Handle>;
};
const picker = window as unknown as Picker;
const DXF_TYPES = [{ description: 'DXF drawing', accept: { 'application/dxf': ['.dxf'] } }];

let handle: Handle | null = null;

const isAbort = (e: unknown) => e instanceof DOMException && e.name === 'AbortError';

/** Picks a file: with the API when there is one, otherwise with a throwaway `<input type=file>`. */
async function chooseFile(): Promise<{ text: string; name: string; handle: Handle | null } | null> {
  if (picker.showOpenFilePicker) {
    try {
      const [h] = await picker.showOpenFilePicker({ types: DXF_TYPES, multiple: false });
      const file = await h.getFile();
      return { text: await file.text(), name: h.name, handle: h };
    } catch (e) {
      if (isAbort(e)) return null;
      throw e;
    }
  }
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = '.dxf,application/dxf,image/vnd.dxf';
    input.onchange = async () => {
      const file = input.files?.[0];
      resolve(file ? { text: await file.text(), name: file.name, handle: null } : null);
    };
    input.oncancel = () => resolve(null);
    input.click();
  });
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

/** Asks for a DXF and opens it. Returns what to tell the person, or `null` if they cancelled. */
export async function openDxf(): Promise<{ ok: boolean; message: string } | null> {
  const picked = await chooseFile();
  if (!picked) return null;
  if (!confirmDiscard()) return null;
  const r = engine.openDxf(picked.text);
  if (!r.ok) return { ok: false, message: r.message };
  leave(); // a different cave: stop sharing the old one
  handle = picked.handle;
  cave.fileName = picked.name;
  for (const w of r.warnings) engine.log('warn', 'file', w);
  cave.opened += 1; // the canvas re-reads and re-frames
  changed();
  markSaved();
  return { ok: true, message: `Opened ${picked.name}: ${r.message}` };
}

/** Saves the open cave. `saveAs` always asks where. Returns what to tell the person, or `null` if they cancelled. */
export async function saveDxf(saveAs = false): Promise<string | null> {
  const text = engine.saveDxf(cave.adjusted);
  if (!text) return 'Nothing to save: no cave is open.';
  try {
    if (picker.showSaveFilePicker) {
      if (!handle || saveAs) handle = await picker.showSaveFilePicker({ suggestedName: cave.fileName, types: DXF_TYPES });
      const w = await handle.createWritable();
      await w.write(text);
      await w.close();
      cave.fileName = handle.name;
      markSaved();
      return `Saved ${handle.name}`;
    }
  } catch (e) {
    if (isAbort(e)) return null;
    throw e;
  }
  // No File System Access API: hand the browser a copy to download.
  const url = URL.createObjectURL(new Blob([text], { type: 'application/dxf' }));
  const a = document.createElement('a');
  a.href = url;
  a.download = cave.fileName;
  a.click();
  URL.revokeObjectURL(url);
  markSaved();
  return `Downloaded ${cave.fileName}`;
}

/** The next demo or file that opens starts with no remembered file to save back to. */
export function forgetFile() {
  handle = null;
}
