// Desktop installers for members: the files listed in DOWNLOADS_DIR/downloads.json (written by
// deploy/update-downloads.sh), and nothing else in that folder. Also the desktop app's own updates: the
// files listed in DOWNLOADS_DIR/update.json, and the question the app asks for them,
// /api/desktop-update/<os>/<arch>/<its version> (the Tauri updater's URL shape).
//
// Test builds ("dev" channel): admins only. They live in a folder beside DOWNLOADS_DIR (`<dir>-dev`, written by
// `deploy/update-downloads.sh --dev`) with the same two lists, and are served at /downloads/dev/<file>. An admin's
// app that asks with `?channel=dev` is offered the newer of the test build and the regular one, so a test build
// never strands it behind a later regular release; everyone else never sees the test builds at all.
import type { IncomingMessage, ServerResponse } from 'node:http';
import { createReadStream } from 'node:fs';
import { readFile, stat } from 'node:fs/promises';
import { join } from 'node:path';
import type { Db } from './db.ts';
import * as M from './members.ts';
import type { RequestLike } from './identity.ts';
import { send } from './http.ts';

export type DownloadsDeps = { db: Db; adminEmail: string; dir: string; devDir?: string; whoIs: (r: RequestLike) => Promise<M.Identity | null> };
export type Manifest = { version: string; builtAt: string; files: Record<string, string> };
/** The newest desktop update: per platform (`darwin-aarch64`, `windows-x86_64`, ...) its file and signature. */
export type UpdateManifest = { version: string; notes?: string; pubDate?: string; platforms: Record<string, { file: string; signature: string }> };

async function updateManifest(dir: string): Promise<UpdateManifest | null> {
  if (!dir) return null;
  try {
    return JSON.parse(await readFile(join(dir, 'update.json'), 'utf8')) as UpdateManifest;
  } catch {
    return null;
  }
}

/** Compares dotted versions (`0.1.42` against `0.1.9`) numerically: negative when `a` is older. */
export function compareVersions(a: string, b: string): number {
  const parts = (v: string) => v.split(/[.+-]/).slice(0, 3).map((n) => Number.parseInt(n, 10) || 0);
  const [x, y] = [parts(a), parts(b)];
  for (let i = 0; i < 3; i++) if ((x[i] ?? 0) !== (y[i] ?? 0)) return (x[i] ?? 0) - (y[i] ?? 0);
  return 0;
}

/** The update for `<os>-<arch>`, as the Tauri updater wants it, or null when there is none newer than `current`. */
export function updateFor(u: UpdateManifest, os: string, arch: string, current: string, origin: string, prefix = '/downloads/') {
  if (compareVersions(current, u.version) >= 0) return null;
  const p = u.platforms[`${os}-${arch}`];
  if (!p?.file || !p.signature) return null;
  return { version: u.version, notes: u.notes ?? '', pub_date: u.pubDate ?? undefined, url: `${origin}${prefix}${encodeURIComponent(p.file)}`, signature: p.signature };
}

/** The test-build folder: `devDir`, or `<dir>-dev` beside the regular one. */
const devDirOf = (d: DownloadsDeps) => d.devDir ?? (d.dir ? `${d.dir.replace(/\/+$/, '')}-dev` : '');

/** A file a manifest pair lists (installers and update files), safe to join onto its folder. */
async function listed(dir: string, name: string): Promise<boolean> {
  if (!name || name.includes('/') || name.includes('\\') || name.includes('..')) return false;
  const m = await manifest(dir);
  const updates = Object.values((await updateManifest(dir))?.platforms ?? {}).map((p) => p.file);
  return Object.values(m?.files ?? {}).includes(name) || updates.includes(name);
}

function sendFile(res: ServerResponse, dir: string, name: string, size: number) {
  res.writeHead(200, { 'Content-Type': 'application/octet-stream', 'Content-Length': size, 'Content-Disposition': `attachment; filename="${name.replace(/"/g, '')}"`, 'Cache-Control': 'private, max-age=300' });
  createReadStream(join(dir, name)).on('error', () => res.destroy()).pipe(res);
}

/** Where this request came in (behind the edge proxy: the host and scheme it forwarded). */
function originOf(req: IncomingMessage): string {
  const h = (k: string) => String(req.headers[k] ?? '').split(',')[0].trim();
  const host = h('x-forwarded-host') || h('host');
  const proto = h('x-forwarded-proto') || 'https';
  return `${proto}://${host}`;
}

async function manifest(dir: string): Promise<Manifest | null> {
  if (!dir) return null;
  try {
    return JSON.parse(await readFile(join(dir, 'downloads.json'), 'utf8')) as Manifest;
  } catch {
    return null;
  }
}

export async function handleDownloads(d: DownloadsDeps, req: IncomingMessage, res: ServerResponse): Promise<void> {
  const [path, query = ''] = (req.url ?? '/').split('?');
  const who = await d.whoIs(req as unknown as RequestLike);
  const update = path.startsWith('/api/desktop-update/');
  const api = path === '/api/downloads' || update;
  if (!who) {
    if (api) return send(res, 401, { error: 'signed-out' });
    res.writeHead(302, { Location: '/signin?return=' + encodeURIComponent(req.url ?? '/'), 'Cache-Control': 'no-store' }).end();
    return;
  }
  if (M.mayLoadApp(d.db, who, { adminEmail: d.adminEmail, now: Date.now() }) !== 'yes') return send(res, 403, { error: 'not-a-member' });
  const admin = M.isAdmin(d.db, who.email);
  const devDir = devDirOf(d);
  if (update) {
    const [os, arch, current] = path.slice('/api/desktop-update/'.length).split('/').map((x) => decodeURIComponent(x));
    const origin = originOf(req);
    const ok = !!(os && arch && current);
    const u = await updateManifest(d.dir);
    let answer = u && ok ? updateFor(u, os, arch, current, origin) : null;
    if (admin && new URLSearchParams(query).get('channel') === 'dev') {
      const t = await updateManifest(devDir);
      const test = t && ok ? updateFor(t, os, arch, current, origin, '/downloads/dev/') : null;
      if (test && (!answer || compareVersions(test.version, answer.version) > 0)) answer = test;
    }
    if (!answer) return void res.writeHead(204, { 'Cache-Control': 'no-store' }).end();
    return send(res, 200, answer);
  }
  if (path === '/api/downloads') {
    const m = await manifest(d.dir);
    const dev = admin ? await manifest(devDir) : null;
    if (!m && !dev) return send(res, 404, { error: 'not-found' });
    return send(res, 200, { ...(m ?? { version: '', builtAt: '', files: {} }), ...(dev ? { dev } : {}) });
  }
  let name: string;
  try {
    name = decodeURIComponent(path.slice('/downloads/'.length));
  } catch {
    return send(res, 404, { error: 'not-found' });
  }
  let dir = d.dir;
  if (name.startsWith('dev/')) {
    // test builds: admins only; anyone else is told there is nothing there
    if (!admin) return send(res, 404, { error: 'not-found' });
    dir = devDir;
    name = name.slice('dev/'.length);
  }
  // An empty name would be the folder itself (the manifest lists '' for a platform with no build).
  if (!dir || !(await listed(dir, name))) return send(res, 404, { error: 'not-found' });
  const st = await stat(join(dir, name)).catch(() => null);
  if (!st?.isFile()) return send(res, 404, { error: 'not-found' });
  sendFile(res, dir, name, st.size);
}
