// Serves the built web app (apps/web/dist). index.html and build-info.json are never cached, hashed
// assets are cached forever, and any unknown path gets index.html so links into the app work.
import { readFile } from 'node:fs/promises';
import { extname, join, normalize } from 'node:path';
import type { IncomingMessage, ServerResponse } from 'node:http';

const TYPES: Record<string, string> = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.wasm': 'application/wasm', '.svg': 'image/svg+xml', '.png': 'image/png', '.ico': 'image/x-icon', '.woff2': 'font/woff2', '.webmanifest': 'application/manifest+json' };

/** Answers one GET for a file under `root`. */
export async function serveStatic(root: string, req: IncomingMessage, res: ServerResponse): Promise<void> {
  const path = decodeURIComponent(new URL(req.url ?? '/', 'http://x').pathname);
  if (path.split('/').includes('..')) {
    res.writeHead(400).end();
    return;
  }
  const rel = normalize(path).replace(/^\/+/, '');
  for (const candidate of rel && !rel.endsWith('/') ? [rel, 'index.html'] : ['index.html']) {
    try {
      const body = await readFile(join(root, candidate));
      const immutable = candidate.startsWith('assets/');
      res.writeHead(200, { 'Content-Type': TYPES[extname(candidate)] ?? 'application/octet-stream', 'Cache-Control': immutable ? 'public, max-age=31536000, immutable' : 'no-store' });
      res.end(body);
      return;
    } catch {
      if (candidate.startsWith('assets/')) break; // a missing asset is a 404, not the app
    }
  }
  res.writeHead(404, { 'Cache-Control': 'no-store' }).end();
}
