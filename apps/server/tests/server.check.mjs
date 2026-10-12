// Checks the server with real sockets: static files (and that nothing outside the folder is served),
// and that a room's ops survive a restart.
import { mkdtempSync, mkdirSync, writeFileSync, appendFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { WebSocket } from 'ws';
import { createCaveServer } from '../server.mjs';
import { createRoomStore } from '../room-store.mjs';

const tmp = mkdtempSync(path.join(tmpdir(), 'cave-server-'));
const staticDir = path.join(tmp, 'dist');
const dataDir = path.join(tmp, 'data');
mkdirSync(path.join(staticDir, 'assets'), { recursive: true });
writeFileSync(path.join(staticDir, 'index.html'), '<h1>cave</h1>');
writeFileSync(path.join(staticDir, 'assets', 'a.wasm'), 'wasm');
writeFileSync(path.join(tmp, 'secret.txt'), 'secret');

const eq = (a, b, what) => {
  if (JSON.stringify(a) !== JSON.stringify(b)) throw new Error(`${what}: got ${JSON.stringify(a)}, wanted ${JSON.stringify(b)}`);
};

async function start() {
  const store = createRoomStore(dataDir);
  const { http } = createCaveServer({ staticDir, store });
  await new Promise((r) => http.listen(0, '127.0.0.1', r));
  return { http, store, base: `127.0.0.1:${http.address().port}` };
}
const client = (base) =>
  new Promise((resolve) => {
    const ws = new WebSocket(`ws://${base}/sync`);
    const got = [];
    const waiters = [];
    ws.on('message', (d) => {
      got.push(JSON.parse(String(d)));
      waiters.splice(0).forEach((w) => w());
    });
    const next = async (t) => {
      for (;;) {
        const i = got.findIndex((m) => m.t === t);
        if (i >= 0) return got.splice(i, 1)[0];
        await new Promise((r) => waiters.push(r));
      }
    };
    ws.on('open', () => resolve({ ws, next, send: (m) => ws.send(JSON.stringify(m)) }));
  });
// fetch normalises ../ away, so raw requests are used for the traversal checks
const raw = (base, requestPath) =>
  new Promise((resolve, reject) => {
    import('node:net').then(({ connect }) => {
      const [host, port] = base.split(':');
      const s = connect(Number(port), host, () => s.write(`GET ${requestPath} HTTP/1.1\r\nHost: x\r\nConnection: close\r\n\r\n`));
      let out = '';
      s.on('data', (d) => (out += d));
      s.on('end', () => resolve(Number(out.slice(9, 12))));
      s.on('error', reject);
    });
  });

let s = await start();
let r = await fetch(`http://${s.base}/`);
eq([r.status, await r.text(), r.headers.get('cache-control')], [200, '<h1>cave</h1>', 'no-cache'], 'index is served and never cached');
r = await fetch(`http://${s.base}/assets/a.wasm`);
eq([r.status, r.headers.get('content-type'), r.headers.get('cache-control')], [200, 'application/wasm', 'public, max-age=31536000, immutable'], 'wasm type, hashed assets immutable');
eq((await fetch(`http://${s.base}/nope`)).status, 404, 'missing file');
eq((await fetch(`http://${s.base}/healthz`)).status, 200, 'health check');
eq((await fetch(`http://${s.base}/`, { method: 'POST' })).status, 405, 'read-only');
// the URL parser flattens ../ to a path inside the folder (404); the folder check is a second guard
for (const p of ['/../secret.txt', '/%2e%2e/secret.txt', '/assets/../../secret.txt']) {
  const code = await raw(s.base, p);
  eq([code === 403 || code === 404], [true], `cannot climb out of the folder: ${p}`);
}

const a = await client(s.base);
a.send({ t: 'join', room: '../../etc/passwd', id: 'a', name: 'A' });
eq((await a.next('welcome')).seed, true, 'empty room is seeded by its first device');
a.send({ t: 'ops', ops: [{ n: 1 }, { n: 2 }] });
a.send({ t: 'ops', ops: [{ n: 3 }] });
await new Promise((r) => setTimeout(r, 100));
await s.store.flush();
eq(readdirSync(dataDir).every((f) => /^[0-9a-f]{32}\.jsonl$/.test(f)), true, 'room names never become paths');
a.ws.close();
await new Promise((r) => s.http.close(r));

// a crash mid-write leaves a torn last line: it is dropped, the rest kept
appendFileSync(path.join(dataDir, readdirSync(dataDir)[0]), '{"room":"x","ops":[{"n"');

s = await start(); // "restart"
const b = await client(s.base);
b.send({ t: 'join', room: '../../etc/passwd', id: 'b', name: 'B' });
const w = await b.next('welcome');
eq([w.seed, w.log], [false, [{ n: 1 }, { n: 2 }, { n: 3 }]], 'after a restart the room still has its ops');
b.send({ t: 'ops', ops: [{ n: 4 }] });
await new Promise((r) => setTimeout(r, 100));
await s.store.flush();
b.ws.close();
await new Promise((r) => s.http.close(r));
eq(createRoomStore(dataDir).load('../../etc/passwd').length, 4, 'new ops are added after the torn line, which stays ignored');

rmSync(tmp, { recursive: true, force: true });
console.log('server checks ok');
