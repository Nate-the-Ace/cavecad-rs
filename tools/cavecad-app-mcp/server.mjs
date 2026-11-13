#!/usr/bin/env node
// cavecad-app: an MCP server (stdio) that drives a running TEST BUILD of the CaveCAD desktop app through its debug
// bridge (apps/desktop/src-tauri/src/bridge.rs). Everything goes through the page, never the mouse or keyboard,
// so it works while someone else uses the computer and the window sits behind theirs.
//
//   claude mcp add --scope user cavecad-app -- node <repo>/tools/cavecad-app-mcp/server.mjs
//
// The app writes its port and key to <app data>/debug-bridge.json when it starts (test builds only); this reads it
// on every call, so restarting the app needs nothing here. CAVECAD_BRIDGE_FILE overrides where it looks.
// No dependencies: Node 18+.

import { readFileSync, mkdtempSync, rmSync } from 'node:fs';
import { homedir, tmpdir, platform } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';
import { createInterface } from 'node:readline';

const ID = 'app.cavecad.desktop';
const bridgeFile = () =>
  process.env.CAVECAD_BRIDGE_FILE ||
  (platform() === 'darwin'
    ? join(homedir(), 'Library', 'Application Support', ID, 'debug-bridge.json')
    : platform() === 'win32'
      ? join(process.env.APPDATA || join(homedir(), 'AppData', 'Roaming'), ID, 'debug-bridge.json')
      : join(process.env.XDG_DATA_HOME || join(homedir(), '.local', 'share'), ID, 'debug-bridge.json'));

const NOT_RUNNING = 'No CaveCAD test build is answering. Start a test build of the desktop app (the debug bridge is only in test builds).';

async function call(op, body = {}) {
  let info;
  try {
    info = JSON.parse(readFileSync(bridgeFile(), 'utf8'));
  } catch {
    throw new Error(NOT_RUNNING);
  }
  let r;
  try {
    r = await fetch(`http://127.0.0.1:${info.port}/${op}`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json', 'X-Bridge-Key': info.key },
      body: JSON.stringify(body),
      signal: AbortSignal.timeout((body.timeoutMs ?? 15000) + 5000),
    });
  } catch {
    throw new Error(NOT_RUNNING);
  }
  const j = await r.json().catch(() => ({ error: `bad answer (${r.status})` }));
  if (!r.ok) throw new Error(j.error || `bridge answered ${r.status}`);
  return j;
}

/** Runs code in the page; returns the parsed value or throws the page's error. */
async function evalIn(code, timeoutMs) {
  const j = await call('eval', { code, timeoutMs });
  if (!j.ok) throw new Error(j.error);
  if (j.value === 'undefined') return undefined;
  try {
    return JSON.parse(j.value);
  } catch {
    return j.value;
  }
}

const B = 'window.__cavecadBridge';
const q = (s) => JSON.stringify(s);
const text = (v) => ({ content: [{ type: 'text', text: typeof v === 'string' ? v : JSON.stringify(v, null, 2) }] });

const TOOLS = [
  {
    name: 'cavecad_app_status',
    description: 'Whether a CaveCAD desktop test build is running, its version, and its window state. Call first.',
    inputSchema: { type: 'object', properties: {} },
    run: async () => text(await call('status')),
  },
  {
    name: 'cavecad_app_eval',
    description:
      'Runs JavaScript in the CaveCAD page (async function body; `return` the result, or give one expression). Results come back as JSON. window.__cavecadBridge has logs/find/click/type/key/text helpers.',
    inputSchema: {
      type: 'object',
      properties: { code: { type: 'string' }, timeoutMs: { type: 'number', description: 'default 15000' } },
      required: ['code'],
    },
    run: async ({ code, timeoutMs }) => text((await evalIn(code, timeoutMs)) ?? 'undefined'),
  },
  {
    name: 'cavecad_app_screenshot',
    description: 'A picture of the CaveCAD window as it is now, even when other windows cover it (macOS).',
    inputSchema: { type: 'object', properties: { maxWidth: { type: 'number', description: 'default 1400' } } },
    run: async ({ maxWidth = 1400 }) => {
      const dir = mkdtempSync(join(tmpdir(), 'cavecad-shot-'));
      try {
        const file = join(dir, 'shot.png');
        await call('screenshot', { path: file });
        if (platform() === 'darwin') execFileSync('/usr/bin/sips', ['-Z', String(maxWidth), file], { stdio: 'ignore' });
        return { content: [{ type: 'image', data: readFileSync(file).toString('base64'), mimeType: 'image/png' }] };
      } finally {
        rmSync(dir, { recursive: true, force: true });
      }
    },
  },
  {
    name: 'cavecad_app_logs',
    description: 'The page log (console, uncaught errors), newest last. `level` filters, e.g. "error,warn,uncaught,unhandled".',
    inputSchema: {
      type: 'object',
      properties: { level: { type: 'string' }, last: { type: 'number', description: 'how many, default 100' }, sinceMs: { type: 'number', description: 'epoch ms' } },
    },
    run: async ({ level = '', last = 100, sinceMs = 0 }) => {
      const rows = await evalIn(`return ${B}.logs(${Number(sinceMs) || 0}, ${q(level)}).slice(-${Math.max(1, Number(last) || 100)})`);
      return text(rows.map((e) => `${new Date(e.t).toISOString().slice(11, 23)} ${e.level.padEnd(9)} ${e.text}`).join('\n') || '(nothing)');
    },
  },
  {
    name: 'cavecad_app_find',
    description: 'Visible buttons, tabs, fields and links whose label contains `text`, with where they are.',
    inputSchema: { type: 'object', properties: { text: { type: 'string' }, selector: { type: 'string' } }, required: ['text'] },
    run: async ({ text: t, selector }) => text(await evalIn(`return ${B}.find(${q(t)}${selector ? ', ' + q(selector) : ''})`)),
  },
  {
    name: 'cavecad_app_click',
    description: 'Presses the visible control whose label contains `text` (shortest match first; `nth` picks another). Through the page: no mouse moves.',
    inputSchema: { type: 'object', properties: { text: { type: 'string' }, nth: { type: 'number' }, selector: { type: 'string' } }, required: ['text'] },
    run: async ({ text: t, nth = 0, selector }) => text(await evalIn(`return ${B}.click(${q(t)}, ${Number(nth) || 0}${selector ? ', ' + q(selector) : ''})`)),
  },
  {
    name: 'cavecad_app_type',
    description: 'Fills the field whose label, placeholder or name contains `field` with `value`.',
    inputSchema: { type: 'object', properties: { field: { type: 'string' }, value: { type: 'string' } }, required: ['field', 'value'] },
    run: async ({ field, value }) => text(await evalIn(`return ${B}.type(${q(field)}, ${q(value)})`)),
  },
  {
    name: 'cavecad_app_key',
    description: 'Sends a key (Escape, Enter, ArrowDown, ...) to whatever has focus in the page.',
    inputSchema: { type: 'object', properties: { key: { type: 'string' } }, required: ['key'] },
    run: async ({ key }) => text(await evalIn(`return ${B}.key(${q(key)})`)),
  },
  {
    name: 'cavecad_app_text',
    description: 'All visible text in the window: a quick read of what is on screen without a picture.',
    inputSchema: { type: 'object', properties: { max: { type: 'number' } } },
    run: async ({ max = 20000 }) => text(await evalIn(`return ${B}.text(${Number(max) || 20000})`)),
  },
];

// --- MCP over stdio: one JSON-RPC message per line ---
const send = (m) => process.stdout.write(JSON.stringify(m) + '\n');
const rl = createInterface({ input: process.stdin });
rl.on('line', async (line) => {
  let m;
  try {
    m = JSON.parse(line);
  } catch {
    return;
  }
  const reply = (result) => send({ jsonrpc: '2.0', id: m.id, result });
  const fail = (code, message) => send({ jsonrpc: '2.0', id: m.id, error: { code, message } });
  if (m.id === undefined) return; // notifications
  switch (m.method) {
    case 'initialize':
      return reply({
        protocolVersion: m.params?.protocolVersion ?? '2025-06-18',
        capabilities: { tools: {} },
        serverInfo: { name: 'cavecad-app', version: '0.1.0' },
        instructions: 'Drives a running CaveCAD desktop TEST BUILD through its debug bridge. Call cavecad_app_status first.',
      });
    case 'ping':
      return reply({});
    case 'tools/list':
      return reply({ tools: TOOLS.map(({ run, ...t }) => t) });
    case 'tools/call': {
      const tool = TOOLS.find((t) => t.name === m.params?.name);
      if (!tool) return fail(-32602, `unknown tool ${m.params?.name}`);
      try {
        return reply(await tool.run(m.params?.arguments ?? {}));
      } catch (e) {
        return reply({ content: [{ type: 'text', text: String(e?.message ?? e) }], isError: true });
      }
    }
    default:
      return fail(-32601, `unknown method ${m.method}`);
  }
});
