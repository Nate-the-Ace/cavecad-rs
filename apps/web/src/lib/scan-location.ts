// Where the scans folder really is on the person's computer. A web page cannot open Finder or
// Explorer, and the browser never says where a picked folder lives, so the person tells the app once
// (a path, kept in this browser only) and "Reveal" hands back the full real path of a page or folder to
// paste into Finder (Go to Folder) or Explorer (address bar). It is never stored in the cave or sent
// to a room: it is a path on this machine.

export type Os = 'mac' | 'windows' | 'other';

/** Which kind of computer this is, from what the browser says about itself. */
export function osOf(platform: string, userAgent: string): Os {
  const s = `${platform} ${userAgent}`.toLowerCase();
  if (/mac|iphone|ipad/.test(s)) return 'mac';
  if (/win/.test(s)) return 'windows';
  return 'other';
}

/** The words for the menu entry. */
export const revealLabel = (os: Os): string => (os === 'mac' ? 'Reveal in Finder' : os === 'windows' ? 'Show in Explorer' : 'Show in file manager');

/** What to do with the copied path. */
export function revealHint(os: Os): string {
  if (os === 'mac') return 'In Finder press Shift+Cmd+G, paste, and press Return: it opens the folder with the page selected.';
  if (os === 'windows') return 'In Explorer click the address bar, paste, and press Enter.';
  return 'Paste it into your file manager.';
}

/** How to copy a folder's path, for the box that asks where the folder lives. */
export function copyPathHint(os: Os): string {
  if (os === 'mac') return 'In Finder, right-click the folder, hold Option, and choose “Copy … as Pathname”.';
  if (os === 'windows') return 'In Explorer, hold Shift, right-click the folder, and choose “Copy as path”.';
  return 'Copy the folder’s full path from your file manager.';
}

/** Cleans what was pasted: no surrounding quotes or spaces, no trailing separator. Empty if nothing usable. */
export function cleanRoot(text: string): string {
  let t = text.trim().replace(/^["']|["']$/g, '').trim();
  if (t.length > 1) t = t.replace(/[\\/]+$/, '');
  return /[\\/]/.test(t) ? t : '';
}

/** The full path of a page or folder inside the scans folder (the separator follows the root's own). */
export function fullPath(root: string, relative: string): string {
  const back = root.includes('\\') && !root.includes('/');
  const sep = back ? '\\' : '/';
  const rel = relative.split('/').filter(Boolean).join(sep);
  return rel ? `${root}${sep}${rel}` : root;
}
