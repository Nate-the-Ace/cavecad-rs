// The open cave's scanned pages, kept with the cave on the CaveCAD server: what is added in the Scans panel is
// uploaded, and every device of the owner, and everyone the cave is shared with, sees the same pages. A page is
// known by its path (the path trips link it by) and fetched the first time something shows it; this browser keeps
// a copy of each page it fetched, by its SHA-256, so it is fetched once.
import { engine } from './engine';
import { isSampleCave, room, serverAccess, signedIn } from './room.svelte';
import { cave as openCave } from './cave.svelte';
import type { ServerAccess } from './room-client';
import { scanKind } from './scan-tree';

/** One page on the server. */
export type CavePage = { path: string; sha: string; size: number; type: string; addedAt: number };

/** The largest page the server takes. */
export const MAX_PAGE_BYTES = 25 * 1024 * 1024;

export const pages = $state({
  /** The cave whose pages are listed (its room id), or ''. */
  cave: '',
  /** The pages on the server, by path. */
  list: [] as CavePage[],
  /** Pages uploaded so far and in all, while an upload runs. */
  sent: 0,
  total: 0,
  /** Why the last upload, listing or removal did not work, or ''. */
  problem: '',
});

/** Why pages cannot be added to the open cave right now, or '' when they can. */
export function cannotAdd(): string {
  if (!openCave.hasCave) return 'Open or start a cave first: scans are kept with a cave.';
  if (!signedIn()) return 'Sign in to add scans: they are kept with the cave on the CaveCAD server.';
  if (isSampleCave()) return 'Sample caves can’t keep scans. Start or open your own cave, then add them.';
  if (room.role === 'watcher') return 'You can look at this cave’s scans but not add them.';
  if (room.status === 'error') return 'This cave is not syncing, so scans cannot be added to it now.';
  if (!engine.roomId()) {
    if (room.status === 'offline') return 'Can’t reach the CaveCAD server, so scans can’t be added yet. Try again once it shows Synced.';
    return 'This cave is still being put on the CaveCAD server. Try again in a moment.';
  }
  return '';
}

/**
 * Waits (up to `ms`) for the open cave to be on the server, when the only thing in the way is that it is still
 * being put there. Gives back what `cannotAdd` says after waiting.
 */
export async function waitUntilAddable(ms = 20_000): Promise<string> {
  const end = Date.now() + ms;
  for (;;) {
    const no = cannotAdd();
    if (!no || engine.roomId() || Date.now() >= end) return no;
    if (!signedIn() || isSampleCave() || !openCave.hasCave || room.status === 'error' || room.role === 'watcher') return no;
    await new Promise((r) => setTimeout(r, 400));
  }
}

const pathOf = (cave: string, path: string) => `/api/caves/${cave}/scans?path=${encodeURIComponent(path)}`;
const headersOf = (a: ServerAccess, type?: string): Record<string, string> => ({ ...(type ? { 'Content-Type': type } : {}), 'X-Cave-Device': a.device.id, ...(a.headers?.() ?? {}) });

/** Reads the open cave's pages from the server again. */
export async function refreshPages(): Promise<void> {
  const cave = engine.roomId();
  if (!cave) {
    pages.cave = '';
    pages.list = [];
    return;
  }
  const a = await serverAccess();
  if (!a) return;
  try {
    const res = await a.fetch(`${a.base}/api/caves/${cave}/scans`, { headers: headersOf(a), credentials: 'same-origin' });
    if (!res.ok) return; // a cave not on the server yet: no pages
    const got = (await res.json()) as { scans: CavePage[] };
    if (engine.roomId() !== cave) return; // another cave opened meanwhile
    pages.cave = cave;
    pages.list = got.scans;
  } catch {
    // offline: the list stays as it was
  }
}

/**
 * Uploads pages to the open cave (`path` → file), three at a time, keeping the folder paths. Gives back the paths
 * kept; the rest are named in `pages.problem`.
 */
export async function uploadPages(found: Map<string, File>): Promise<string[]> {
  const no = await waitUntilAddable();
  if (no) {
    pages.problem = no;
    return [];
  }
  const cave = engine.roomId();
  const a = await serverAccess();
  if (!a) {
    pages.problem = 'Sign in to add scans: they are kept with the cave on the CaveCAD server.';
    return [];
  }
  const todo = [...found].filter(([p]) => scanKind(p));
  const tooBig = todo.filter(([, f]) => f.size > MAX_PAGE_BYTES).map(([p]) => p);
  const queue = todo.filter(([, f]) => f.size <= MAX_PAGE_BYTES);
  pages.problem = '';
  pages.sent = 0;
  pages.total = queue.length;
  const kept: string[] = [];
  const failed: string[] = [];
  const one = async () => {
    for (let next = queue.shift(); next; next = queue.shift()) {
      const [path, file] = next;
      try {
        const res = await a.fetch(a.base + pathOf(cave, path), { method: 'PUT', headers: headersOf(a, 'application/octet-stream'), body: file, credentials: 'same-origin' });
        if (!res.ok) throw new Error(String(res.status));
        const got = (await res.json()) as { scan: CavePage };
        await keepLocal(got.scan.sha, file); // no need to fetch back what was just sent
        kept.push(path);
      } catch {
        failed.push(path);
      }
      pages.sent += 1;
    }
  };
  await Promise.all([one(), one(), one()]);
  pages.total = 0;
  const said: string[] = [];
  if (failed.length) said.push(`${failed.length} page${failed.length === 1 ? '' : 's'} could not be uploaded (${failed.slice(0, 3).join(', ')}${failed.length > 3 ? '…' : ''}). Add them again to retry.`);
  if (tooBig.length) said.push(`${tooBig.length} file${tooBig.length === 1 ? ' is' : 's are'} over 25 MB and ${tooBig.length === 1 ? 'was' : 'were'} not added (${tooBig.slice(0, 3).join(', ')}).`);
  pages.problem = said.join(' ');
  await refreshPages();
  return kept;
}

/** Takes a page off the open cave, for everyone. A sentence when it could not, or ''. */
export async function removePage(path: string): Promise<string> {
  const cave = engine.roomId();
  const a = await serverAccess();
  if (!cave || !a) return 'Sign in to change this cave’s scans.';
  try {
    const res = await a.fetch(a.base + pathOf(cave, path), { method: 'DELETE', headers: headersOf(a, 'application/json'), credentials: 'same-origin' });
    if (!res.ok) return res.status === 403 ? 'You can look at this cave’s scans but not change them.' : 'The page could not be removed. Try again in a moment.';
  } catch {
    return 'Can’t reach the CaveCAD server. Check you are online, then try again.';
  }
  await refreshPages();
  return '';
}

// This browser's copies of fetched pages, by SHA-256 (the private storage the page can always write).
async function cacheDir(): Promise<FileSystemDirectoryHandle | null> {
  try {
    return await (await navigator.storage.getDirectory()).getDirectoryHandle('cave-pages', { create: true });
  } catch {
    return null;
  }
}
async function keepLocal(sha: string, file: Blob): Promise<void> {
  try {
    const dir = await cacheDir();
    if (!dir) return;
    const w = await (await dir.getFileHandle(sha, { create: true })).createWritable();
    await w.write(file);
    await w.close();
  } catch {
    // no room: the page is fetched again next time
  }
}

/** A page's picture: this browser's copy, else fetched from the server (and kept). Null when it cannot be had. */
export async function fetchPage(page: CavePage): Promise<File | null> {
  const name = page.path.slice(page.path.lastIndexOf('/') + 1);
  try {
    const dir = await cacheDir();
    const got = await (await dir?.getFileHandle(page.sha))?.getFile();
    if (got && got.size === page.size) return new File([got], name, { type: page.type });
  } catch {
    // not kept here yet
  }
  const cave = pages.cave;
  const a = await serverAccess();
  if (!cave || !a) return null;
  try {
    const res = await a.fetch(`${a.base}/api/caves/${cave}/scans/${page.sha}`, { headers: headersOf(a), credentials: 'same-origin' });
    if (!res.ok) return null;
    const blob = await res.blob();
    await keepLocal(page.sha, blob);
    return new File([blob], name, { type: page.type });
  } catch {
    return null;
  }
}
