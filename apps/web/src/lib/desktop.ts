// The desktop app (Tauri) wraps this same web app in a native window and adds what a browser is not
// allowed to do: the folder dialog gives the real path of the scans folder, pages are read straight
// from it, and Reveal opens Finder or Explorer on the real file. In a browser none of this exists
// and `isDesktop()` is false; every caller keeps its browser way of doing things.

type Invoke = (cmd: string, args?: Record<string, unknown>) => Promise<unknown>;

const invoke = (): Invoke | undefined => (window as unknown as { __TAURI_INTERNALS__?: { invoke: Invoke } }).__TAURI_INTERNALS__?.invoke;

/** True inside the desktop app. */
export const isDesktop = (): boolean => invoke() !== undefined;

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const f = invoke();
  if (!f) throw new Error('not in the desktop app');
  return (await f(cmd, args)) as T;
}

/** Asks which folder, with the system's own dialog. The real path, or `null` if cancelled. */
export const pickFolderNative = (): Promise<string | null> => call<string | null>('pick_folder');

/** The pictures and PDFs under a folder. */
export const listScansNative = (root: string): Promise<{ path: string; size: number }[]> => call('list_scans', { root });

/** One page's bytes, read from the folder. */
export const readScanNative = (root: string, rel: string): Promise<ArrayBuffer> => call('read_scan', { root, rel });

/** Opens Finder or Explorer on the real file, selected (an empty `rel` is the folder itself). */
export const revealNative = (root: string, rel: string): Promise<void> => call('reveal', { root, rel });

const TYPES: Record<string, string> = { png: 'image/png', jpg: 'image/jpeg', jpeg: 'image/jpeg', webp: 'image/webp', gif: 'image/gif', bmp: 'image/bmp', svg: 'image/svg+xml', avif: 'image/avif', pdf: 'application/pdf' };

/** The media type of a page from its name. */
export const typeOf = (path: string): string => TYPES[path.slice(path.lastIndexOf('.') + 1).toLowerCase()] ?? 'application/octet-stream';

/** The last part of a path, whichever way it is written. */
export const baseName = (path: string): string => path.replace(/[\\/]+$/, '').split(/[\\/]/).pop() || path;

/**
 * Join links that open the desktop app (`cavecad://join?...`) reach the page here: the ones that started the app before
 * the page was ready, and each one that arrives while it runs. Does nothing in a browser.
 */
export function listenForLinks(onLink: (url: string) => void): void {
  if (!isDesktop()) return;
  (window as unknown as { __cavecadLink?: (u: string) => void }).__cavecadLink = (url) => {
    onLink(url);
    void call<string[]>('take_links').catch(() => undefined); // it has been seen: the waiting copy is not needed
  };
  void call<string[]>('take_links').then((urls) => urls.forEach(onLink)).catch(() => undefined);
}
