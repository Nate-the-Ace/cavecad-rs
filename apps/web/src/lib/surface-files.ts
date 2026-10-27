// The surface files (aerial photo, elevation grid): downloaded straight from USGS, kept in the browser's
// private storage (OPFS) under a key made from the REQUEST, and packed into the .cavecad file. Keyed by the
// request so undoing a re-download still finds the earlier files, and two caves cut to the same window share them.
// Never log a URL from here: it carries the window, which locates the cave.

import type { SurfacePlan } from './engine';

export type Part = 'photo' | 'elevation';
export const TIMEOUT_MS = 60_000;

/** A stable key for one part of a plan (FNV-1a over the canonical request). */
export function requestKey(plan: Pick<SurfacePlan, 'zone' | 'bounds'> & Partial<SurfacePlan>, part: Part): string {
  const p = part === 'photo' ? plan.photo : plan.elevation;
  const b = plan.bounds;
  const text = [part, plan.zone, b.xmin.toFixed(3), b.ymin.toFixed(3), b.xmax.toFixed(3), b.ymax.toFixed(3), p?.w ?? 0, p?.h ?? 0].join('|');
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) h = Math.imul(h ^ text.charCodeAt(i), 0x01000193) >>> 0;
  return `${part}-${h.toString(16).padStart(8, '0')}`;
}

export const looksLikeJpeg = (b: Uint8Array) => b.length > 3 && b[0] === 0xff && b[1] === 0xd8 && b[2] === 0xff;
export const looksLikeTiff = (b: Uint8Array) => b.length > 4 && ((b[0] === 0x49 && b[1] === 0x49 && b[2] === 42 && b[3] === 0) || (b[0] === 0x4d && b[1] === 0x4d && b[2] === 0 && b[3] === 42));

/** fetch with a deadline. */
export async function fetchWithTimeout(url: string, ms = TIMEOUT_MS): Promise<Uint8Array> {
  const ctl = new AbortController();
  const t = setTimeout(() => ctl.abort(), ms);
  try {
    const r = await fetch(url, { signal: ctl.signal, cache: 'no-store' });
    if (!r.ok) throw new Error(`the service answered ${r.status}`);
    return new Uint8Array(await r.arrayBuffer());
  } catch (e) {
    if ((e as Error).name === 'AbortError') throw new Error(`the download did not finish within ${ms / 1000} seconds`);
    throw e;
  } finally {
    clearTimeout(t);
  }
}

/** Downloads one part and checks it is what was asked for (USGS sends errors as JSON with status 200). */
export async function download(plan: SurfacePlan, part: Part): Promise<Uint8Array> {
  const p = part === 'photo' ? plan.photo : plan.elevation;
  if (!p) throw new Error(`no ${part} in this plan`);
  const bytes = await fetchWithTimeout(p.url);
  if (part === 'photo' ? !looksLikeJpeg(bytes) : !looksLikeTiff(bytes)) throw new Error(`the service sent something that is not ${part === 'photo' ? 'a photograph' : 'an elevation grid'} (usually an error message). Try again.`);
  return bytes;
}

async function dir(): Promise<FileSystemDirectoryHandle | null> {
  try {
    const root = await navigator.storage.getDirectory();
    return await root.getDirectoryHandle('surface', { create: true });
  } catch {
    return null;
  }
}

/** Keeps a part's bytes under its key (OPFS). Quietly does nothing where OPFS is unavailable. */
export async function store(key: string, bytes: Uint8Array): Promise<void> {
  const d = await dir();
  if (!d) return;
  const f = await d.getFileHandle(key, { create: true });
  const w = await f.createWritable();
  await w.write(bytes as BlobPart);
  await w.close();
}

/** A kept part, or null. */
export async function load(key: string): Promise<Uint8Array | null> {
  const d = await dir();
  if (!d) return null;
  try {
    const f = await (await d.getFileHandle(key)).getFile();
    return new Uint8Array(await f.arrayBuffer());
  } catch {
    return null;
  }
}
