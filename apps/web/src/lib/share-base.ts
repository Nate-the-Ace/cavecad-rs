// The address a join link is built on. A link has to open for the person who receives it, so it cannot be this page's own
// address when that is only good on this machine: the desktop app, a file, or a copy running on this computer for
// development. Those links open the hosted copy of CaveCAD instead, which does the joining the same way.

/** Where the hosted copy lives (set when the site is built; this is the default). */
const HOSTED = ((import.meta.env?.VITE_PAGES_URL as string | undefined) ?? 'https://nate-the-ace.github.io/cavecad-rs/').trim();

/** True when this page is the desktop app (or opened from a file), whose own address means nothing to anyone else. */
export function isShell(): boolean {
  const w = window as unknown as Record<string, unknown>;
  return '__TAURI_INTERNALS__' in w || '__TAURI__' in w || location.protocol === 'tauri:' || location.protocol === 'file:' || location.hostname === 'tauri.localhost';
}

/** The address to build a join link on. */
export function shareBase(): string {
  return isShell() ? HOSTED : location.href;
}
