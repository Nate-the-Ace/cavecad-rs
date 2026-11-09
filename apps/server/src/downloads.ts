// Desktop installers for members: the files listed in DOWNLOADS_DIR/downloads.json (written by
// deploy/update-downloads.sh), and nothing else in that folder.
import type { IncomingMessage, ServerResponse } from 'node:http';
import { createReadStream } from 'node:fs';
import { readFile, stat } from 'node:fs/promises';
import { join } from 'node:path';
import type { Db } from './db.ts';
import * as M from './members.ts';
import type { RequestLike } from './identity.ts';
import { send } from './http.ts';

export type DownloadsDeps = { db: Db; adminEmail: string; dir: string; whoIs: (r: RequestLike) => Promise<M.Identity | null> };
export type Manifest = { version: string; builtAt: string; files: Record<string, string> };

async function manifest(dir: string): Promise<Manifest | null> {
  if (!dir) return null;
  try {
    return JSON.parse(await readFile(join(dir, 'downloads.json'), 'utf8')) as Manifest;
  } catch {
    return null;
  }
}

export async function handleDownloads(d: DownloadsDeps, req: IncomingMessage, res: ServerResponse): Promise<void> {
  const path = (req.url ?? '/').split('?')[0];
  const who = await d.whoIs(req as unknown as RequestLike);
  const api = path === '/api/downloads';
  if (!who) {
    if (api) return send(res, 401, { error: 'signed-out' });
    res.writeHead(302, { Location: '/signin?return=' + encodeURIComponent(req.url ?? '/'), 'Cache-Control': 'no-store' }).end();
    return;
  }
  if (M.mayLoadApp(d.db, who, { adminEmail: d.adminEmail, now: Date.now() }) !== 'yes') return send(res, 403, { error: 'not-a-member' });
  const m = await manifest(d.dir);
  if (!m) return send(res, 404, { error: 'not-found' });
  if (api) return send(res, 200, m);
  let name: string;
  try {
    name = decodeURIComponent(path.slice('/downloads/'.length));
  } catch {
    return send(res, 404, { error: 'not-found' });
  }
  // An empty name would be the folder itself (the manifest lists '' for a platform with no build).
  if (!name || !Object.values(m.files ?? {}).includes(name) || name.includes('/') || name.includes('\\') || name.includes('..')) return send(res, 404, { error: 'not-found' });
  const file = join(d.dir, name);
  const st = await stat(file).catch(() => null);
  if (!st?.isFile()) return send(res, 404, { error: 'not-found' });
  res.writeHead(200, { 'Content-Type': 'application/octet-stream', 'Content-Length': st.size, 'Content-Disposition': `attachment; filename="${name.replace(/"/g, '')}"`, 'Cache-Control': 'private, max-age=300' });
  createReadStream(file).on('error', () => res.destroy()).pipe(res);
}
