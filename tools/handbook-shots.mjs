// Photographs each Handbook panel page's panel from a running dev server (capture mode, DEV only) with headless
// Chrome driven over the DevTools protocol, into apps/web/src/handbook/shots/, and records which version of the
// panel's source each shot shows (shots.json).
// Usage (from the repo root): node tools/handbook-shots.mjs http://localhost:5196
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync, mkdirSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const base = process.argv[2] ?? 'http://localhost:5173';
const chrome = process.env.CHROME ?? '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const port = 9300 + Math.floor(Math.random() * 500);
const index = JSON.parse(readFileSync('apps/web/src/handbook/index.json', 'utf8'));
const ws = readFileSync('apps/web/src/lib/workspace.svelte.ts', 'utf8');
const out = 'apps/web/src/handbook/shots';
mkdirSync(out, { recursive: true });

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const proc = spawn(chrome, ['--headless=new', '--hide-scrollbars', '--window-size=1100,700', `--remote-debugging-port=${port}`,
  `--user-data-dir=${mkdtempSync(join(tmpdir(), 'hb-shots-'))}`, '--no-first-run', 'about:blank'], { stdio: 'ignore' });
try {
  let target;
  for (let i = 0; i < 50 && !target; i++) {
    await sleep(200);
    try { target = (await (await fetch(`http://127.0.0.1:${port}/json`)).json()).find((t) => t.type === 'page'); } catch {}
  }
  if (!target) throw new Error('Chrome did not start');
  const sock = new WebSocket(target.webSocketDebuggerUrl);
  await new Promise((r, j) => { sock.onopen = r; sock.onerror = j; });
  let n = 0;
  const pending = new Map();
  sock.onmessage = (m) => { const d = JSON.parse(m.data); pending.get(d.id)?.(d); };
  const send = (method, params = {}) => new Promise((res, rej) => {
    const id = ++n;
    pending.set(id, (d) => (d.error ? rej(new Error(d.error.message)) : res(d.result)));
    sock.send(JSON.stringify({ id, method, params }));
  });
  await send('Page.enable');
  await send('Emulation.setDeviceMetricsOverride', { width: 1100, height: 700, deviceScaleFactor: 1, mobile: false });

  const record = {};
  for (const e of index.filter((x) => x.panel)) {
    const comp = new RegExp(`\\b${e.panel}: \\{ title: '[^']*', component: (\\w+)`).exec(ws)?.[1];
    const imp = comp && new RegExp(`import ${comp} from '\\.\\./([^']+)'`).exec(ws)?.[1];
    if (!imp) { console.log(`skip ${e.panel}: not in PANELS`); continue; }
    const source = `apps/web/src/${imp}`;
    const file = `${e.panel}.png`;
    // a fresh page each time, so no state leaks between shots (the cave is kept in browser storage)
    await send('Page.navigate', { url: 'about:blank' });
    await send('Network.enable');
    await send('Network.clearBrowserCache');
    await send('Page.navigate', { url: `${base}/?handbook-shot=${e.panel}` });
    let ready = false;
    for (let i = 0; i < 150 && !ready; i++) {
      await sleep(400);
      const r = await send('Runtime.evaluate', { expression: "document.body && document.body.dataset.shotReady === '1'" });
      ready = r.result.value === true;
    }
    if (!ready) { console.log(`FAILED ${e.panel}: never became ready`); continue; }
    const shot = await send('Page.captureScreenshot', { format: 'png' });
    writeFileSync(`${out}/${file}`, Buffer.from(shot.data, 'base64'));
    record[file] = { source, hash: createHash('sha256').update(readFileSync(source)).digest('hex') };
    console.log('shot', file);
  }
  writeFileSync('apps/web/src/handbook/shots.json', JSON.stringify(record, null, 2) + '\n');
  console.log(`${Object.keys(record).length} shots`);
  sock.close();
} finally {
  proc.kill();
}
