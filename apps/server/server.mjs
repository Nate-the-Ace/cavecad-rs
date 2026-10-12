// The always-on CaveCAD server: serves the built web app and the live-editing relay at /sync, and
// keeps every room's op log on disk. It never reads an op and never logs cave data.
//
//   PORT       listen port (default 8080)      HOST      listen address (default 0.0.0.0)
//   DATA_DIR   where room logs live (default ./data)
//   STATIC_DIR the built web app (default ../web/dist)
import { createServer } from 'node:http';
import { createReadStream } from 'node:fs';
import { stat } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { attachRelay, createRelay } from '../web/scripts/sync-relay.mjs';
import { createRoomStore } from './room-store.mjs';

const TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.wasm': 'application/wasm',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
  '.woff2': 'font/woff2',
  '.txt': 'text/plain; charset=utf-8',
};

/** Same rules as apps/web/public/_headers: hashed assets are immutable, entry points never cached. */
function cacheControl(urlPath) {
  if (urlPath.startsWith('/assets/')) return 'public, max-age=31536000, immutable';
  if (urlPath === '/build-info.json') return 'no-store';
  return 'no-cache';
}

/** Builds the http server (not yet listening). `store` is where room logs are kept. */
export function createCaveServer({ staticDir, store }) {
  const root = path.resolve(staticDir);
  const relay = createRelay(store);

  const http = createServer(async (req, res) => {
    if (req.method !== 'GET' && req.method !== 'HEAD') return void res.writeHead(405, { Allow: 'GET, HEAD' }).end();
    let urlPath;
    try {
      urlPath = decodeURIComponent(new URL(req.url ?? '/', 'http://x').pathname);
    } catch {
      return void res.writeHead(400).end();
    }
    if (urlPath === '/healthz') return void res.writeHead(200, { 'Content-Type': 'text/plain' }).end('ok');
    if (urlPath.includes('\0')) return void res.writeHead(400).end();
    const file = path.join(root, urlPath.endsWith('/') ? `${urlPath}index.html` : urlPath);
    if (file !== root && !file.startsWith(root + path.sep)) return void res.writeHead(403).end();
    try {
      const info = await stat(file);
      if (!info.isFile()) throw new Error('not a file');
      res.writeHead(200, {
        'Content-Type': TYPES[path.extname(file)] ?? 'application/octet-stream',
        'Content-Length': info.size,
        'Cache-Control': cacheControl(urlPath),
      });
      if (req.method === 'HEAD') return void res.end();
      createReadStream(file).on('error', () => res.destroy()).pipe(res);
    } catch {
      res.writeHead(404, { 'Content-Type': 'text/plain' }).end('Not found');
    }
  });
  attachRelay(http, relay);
  return { http, relay };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const here = path.dirname(fileURLToPath(import.meta.url));
  const port = Number(process.env.PORT ?? 8080);
  const host = process.env.HOST ?? '0.0.0.0';
  const staticDir = process.env.STATIC_DIR ?? path.join(here, '../web/dist');
  const dataDir = process.env.DATA_DIR ?? path.join(process.cwd(), 'data');
  const store = createRoomStore(dataDir);
  const { http } = createCaveServer({ staticDir, store });
  http.listen(port, host, () => console.log(`[cave-server] listening on ${host}:${port}, rooms kept in ${dataDir}`));

  const stop = async () => {
    http.close();
    http.closeAllConnections?.();
    await store.flush();
    process.exit(0);
  };
  process.on('SIGTERM', stop);
  process.on('SIGINT', stop);
}
