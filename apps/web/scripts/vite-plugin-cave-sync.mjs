// Serves the live-editing relay on the dev and preview servers at /sync, so one command
// (`npm run dev`) is all a second device needs: open the laptop's address in its browser.
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { attachRelay, createRelay } from './sync-relay.mjs';

/** Feedback reports (zips posted to /feedback) are kept in this folder, which git ignores. */
const FEEDBACK_DIR = join(process.cwd(), '..', '..', 'feedback');
const MAX_REPORT = 25 * 1024 * 1024;

function takeFeedback(req, res) {
  if (req.method !== 'POST' || req.url?.split('?')[0] !== '/feedback') return false;
  const chunks = [];
  let size = 0;
  req.on('data', (c) => {
    size += c.length;
    if (size > MAX_REPORT) req.destroy();
    else chunks.push(c);
  });
  req.on('end', () => {
    const asked = new URL(req.url, 'http://x').searchParams.get('name') ?? '';
    const name = asked.replace(/[^A-Za-z0-9._-]/g, '').slice(0, 80) || `feedback-${Date.now()}.zip`;
    mkdirSync(FEEDBACK_DIR, { recursive: true });
    writeFileSync(join(FEEDBACK_DIR, name.endsWith('.zip') ? name : `${name}.zip`), Buffer.concat(chunks));
    res.statusCode = 200;
    res.end('ok');
  });
  return true;
}

export default function caveSync() {
  const relay = createRelay();
  const middleware = (req, res, next) => (takeFeedback(req, res) ? undefined : next());
  return {
    name: 'cave-sync',
    configureServer(server) {
      server.middlewares.use(middleware);
      if (server.httpServer) attachRelay(server.httpServer, relay);
    },
    configurePreviewServer(server) {
      server.middlewares.use(middleware);
      if (server.httpServer) attachRelay(server.httpServer, relay);
    },
  };
}
