// CaveCAD's own file: a zip holding the whole cave exactly as it lives in the app, so nothing is
// squeezed into what a CAD drawing has room for.
//
//   manifest.json   what this is, which version wrote it, and which scans are inside
//   cave.json       every trip, shot, note, line, fill, scan placement and mark (the cave's ops)
//   scans/<path>    the scan pages the cave uses, when they are included (optional)
//   surface/photo.jpg, surface/elevation.tif, surface/surface.json
//                   the surface the cave was mapped against (optional)
//
// A newer version of the app always reads an older file: `version` only goes up, and a reader
// ignores what it does not know. A DXF is an export, not how a cave is saved.

import { unzip } from './unzip.ts';
import { zip, type ZipEntry } from './zip.ts';

export const EXT = '.cavecad';
export const VERSION = 1;

export interface SurfaceParts {
  /** The `surface` setting's JSON (what was downloaded). */
  record: string;
  photo: Uint8Array | null;
  elevation: Uint8Array | null;
}

export interface Manifest {
  format: 'cavecad';
  version: number;
  /** The app build that wrote it. */
  app: string;
  name: string;
  savedAt: string;
  /** The scan pages inside, by their path in the scans folder. */
  scans: { path: string; bytes: number }[];
  /** The surface files inside, when included. */
  surface?: { path: string; bytes: number }[];
}

export interface Bundle {
  manifest: Manifest;
  /** The cave's ops, as JSON text. */
  cave: string;
  scans: { path: string; data: Uint8Array }[];
  surface: SurfaceParts | null;
}

const enc = new TextEncoder();
const dec = new TextDecoder();

/** The bytes of a CaveCAD file. */
export function pack(opts: { name: string; app: string; cave: string; scans?: { path: string; data: Uint8Array }[]; extra?: ZipEntry[]; surface?: SurfaceParts | null }): Uint8Array {
  const scans = opts.scans ?? [];
  const sp = opts.surface;
  const surf: ZipEntry[] = sp ? [{ name: 'surface/surface.json', data: enc.encode(sp.record) }, ...(sp.photo ? [{ name: 'surface/photo.jpg', data: sp.photo }] : []), ...(sp.elevation ? [{ name: 'surface/elevation.tif', data: sp.elevation }] : [])] : [];
  const manifest: Manifest = { format: 'cavecad', version: VERSION, app: opts.app, name: opts.name, savedAt: new Date().toISOString(), scans: scans.map((s) => ({ path: s.path, bytes: s.data.length })), ...(surf.length ? { surface: surf.map((e) => ({ path: e.name, bytes: e.data.length })) } : {}) };
  return zip([
    { name: 'manifest.json', data: enc.encode(JSON.stringify(manifest, null, 2)) },
    { name: 'cave.json', data: enc.encode(opts.cave) },
    ...scans.map((s) => ({ name: `scans/${s.path}`, data: s.data })),
    ...surf,
    ...(opts.extra ?? []),
  ]);
}

/** Reads a CaveCAD file, or says why not (`null`: it is not one). A file from a newer version is refused with a reason. */
export async function unpack(bytes: Uint8Array): Promise<Bundle | { error: string } | null> {
  const files = await unzip(bytes);
  if (!files) return null;
  const byName = new Map(files.map((f) => [f.name, f.data]));
  const m = byName.get('manifest.json');
  const cave = byName.get('cave.json');
  if (!m || !cave) return null;
  let manifest: Manifest;
  try {
    manifest = JSON.parse(dec.decode(m));
  } catch {
    return null;
  }
  if (manifest.format !== 'cavecad') return null;
  if (typeof manifest.version !== 'number' || manifest.version > VERSION) return { error: 'This file was saved by a newer CaveCAD. Update the app to open it.' };
  const scans = files.filter((f) => f.name.startsWith('scans/')).map((f) => ({ path: f.name.slice('scans/'.length), data: f.data }));
  const sj = byName.get('surface/surface.json');
  const surface: SurfaceParts | null = sj ? { record: dec.decode(sj), photo: byName.get('surface/photo.jpg') ?? null, elevation: byName.get('surface/elevation.tif') ?? null } : null;
  return { manifest, cave: dec.decode(cave), scans, surface };
}
