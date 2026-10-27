// The desktop app (Tauri) wraps this same web app in a native window and adds what a browser is not
// allowed to do: the folder dialog gives the real path of the scans folder, pages are read straight
// from it, and Reveal opens Finder or Explorer on the real file. In a browser none of this exists
// and `isDesktop()` is false; every caller keeps its browser way of doing things.

type Invoke = (cmd: string, args?: Record<string, unknown> | Uint8Array, options?: { headers: Record<string, string> }) => Promise<unknown>;

const invoke = (): Invoke | undefined => (window as unknown as { __TAURI_INTERNALS__?: { invoke: Invoke } }).__TAURI_INTERNALS__?.invoke;

/** True inside the desktop app. */
export const isDesktop = (): boolean => invoke() !== undefined;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const f = invoke();
  if (!f) throw new Error('not in the desktop app');
  return (await f(cmd, args)) as T;
}

/** A command that takes a file's bytes as they are (not as JSON), with names in headers. */
async function send<T>(cmd: string, bytes: Uint8Array, headers: Record<string, string>): Promise<T> {
  const f = invoke();
  if (!f) throw new Error('not in the desktop app');
  const escaped = Object.fromEntries(Object.entries(headers).map(([k, v]) => [k, encodeURIComponent(v)]));
  return (await f(cmd, bytes, { headers: escaped })) as T;
}

/** The person's Documents folder, or `null`. */
export const documentsFolderNative = (): Promise<string | null> => call('documents_folder');

/** Asks where a new cave's folder goes, starting at `start` (Documents when it is gone). The real path, or `null` if cancelled. */
export const pickCavePlaceNative = (start: string | null): Promise<string | null> => call('pick_cave_place', { start });

/** What the cave's folder would be under `parent`, and what is there already. */
export const lookCavePlaceNative = (parent: string, name: string): Promise<{ folder: string; place: { kind: 'empty' | 'notACave' } | { kind: 'hasCave'; file: string } }> => call('look_cave_place', { parent, name });

/** Makes (or completes) the cave's folder with its standard subfolders. Its path. */
export const makeCaveFolderNative = (parent: string, name: string): Promise<string> => call('make_cave_folder', { parent, name });

/** True when a folder is still where it was. */
export const folderExistsNative = (folder: string): Promise<boolean> => call('folder_exists', { folder });

/** Writes the cave file into its folder, backing up the last copy first. `stamp` is local time as `yyyy-mm-dd hhmm`. */
export const writeCaveNative = (folder: string, file: string, bytes: Uint8Array, stamp: string): Promise<void> => send('write_cave', bytes, { 'x-folder': folder, 'x-file': file, 'x-stamp': stamp });

/** The cave file's bytes from its folder. */
export const readCaveNative = (folder: string, file: string): Promise<ArrayBuffer> => call('read_cave', { folder, file });

/** Writes an export into the cave folder's `exports/`. Where it went. */
export const writeExportNative = (folder: string, file: string, bytes: Uint8Array): Promise<string> => send('write_export', bytes, { 'x-folder': folder, 'x-file': file });

/** Asks which folder, with the system's own dialog. The real path, or `null` if cancelled. */
export const pickFolderNative = (): Promise<string | null> => call<string | null>('pick_folder');

/** The pictures and PDFs under a folder. */
export const listScansNative = (root: string): Promise<{ path: string; size: number }[]> => call('list_scans', { root });

/** One page's bytes, read from the folder. */
export const readScanNative = (root: string, rel: string): Promise<ArrayBuffer> => call('read_scan', { root, rel });

/** Opens Finder or Explorer on the real file, selected (an empty `rel` is the folder itself). */
/** Opens an https address in the system browser (the desktop sign-in). */
export const openUrlNative = (url: string): Promise<void> => call('open_url', { url });

export const revealNative = (root: string, rel: string): Promise<void> => call('reveal', { root, rel });

const TYPES: Record<string, string> = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', webp: 'image/webp', gif: 'image/gif', bmp: 'image/bmp', svg: 'image/svg+xml', avif: 'image/avif', pdf: 'application/pdf' };

/** The media type of a page from its name. */
export const typeOf = (path: string): string => TYPES[path.slice(path.lastIndexOf('.') + 1).toLowerCase()] ?? 'application/octet-stream';

/** The last part of a path, whichever way it is written. */
export const baseName = (path: string): string => path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || path;

/**
 * Links that open the desktop app (`cavecad://auth` after sign-in, and old `cavecad://join?...` links, which show the
 * moved page) reach the page here: the ones that started the app before the page was ready, and each one that arrives
 * while it runs. Does nothing in a browser.
 */
export function listenForLinks(onLink: (url: string) => void): void {
  if (!isDesktop()) return;
  (window as unknown as { __cavecadLink?: (u: string) => void }).__cavecadLink = (url) => {
    onLink(url);
    void call<string[]>('take_links').catch(() => undefined); // it has been seen: the waiting copy is not needed
  };
  void call<string[]>('take_links').then((urls) => urls.forEach(onLink)).catch(() => undefined);
}
