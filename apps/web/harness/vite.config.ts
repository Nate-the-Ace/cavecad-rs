// A way to touch the Survey panel and the sketchpad on any machine, with no Rust build: the real ribbon, panels
// and session code, with the engine replaced by a small in-memory mock (harness/mock-impl.js). Run
// `npm run harness` in apps/web and open http://localhost:5199 (or the Network address on a tablet on the same
// network). What it cannot show: the Map (it needs the real engine) and Bluetooth (use the Simulator).
//
// Nothing the Draw tab shows is copied by hand: the type palette, the tool groups and the plot scales are READ
// from the Rust sources (crates/cave-style, crates/cave-sheet) each time the page loads, so the harness's Draw
// tab and scale list are the app's, and cannot go stale.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, type Plugin } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../../..');
const VIRTUAL = '\0cave-wasm-mock';

const read = (p: string) => fs.readFileSync(path.resolve(root, p), 'utf8');

/** The type palette from cave-style's `PALETTE` table: `s("wall", "Wall", Text, 2.4, Solid, 1.0, 0.4, "CAVE-WALLS")`. */
function palette() {
  const src = read('crates/cave-style/src/lib.rs');
  const start = src.indexOf('pub const PALETTE');
  const table = src.slice(start, src.indexOf('\n];', start));
  const row = /s\(\s*"([^"]+)",\s*"([^"]+)",\s*(\w+),\s*([\d.]+),\s*(\w+),\s*([\d.]+),\s*([\d.]+),\s*"([^"]+)",?\s*\)/g;
  return [...table.matchAll(row)].map((m) => ({ key: m[1], label: m[2], ink: m[3].toLowerCase(), weight: Number(m[4]), linetype: m[5].toLowerCase(), opacity: Number(m[6]), under_opacity: Number(m[7]), layer: m[8] }));
}

/** The Draw tab's groups from `TOOL_GROUPS`. Text and callout are left out: the sketchpad draws lines, and those two place words. */
function toolGroups(styles: ReturnType<typeof palette>) {
  const src = read('crates/cave-style/src/lib.rs');
  const start = src.indexOf('pub const TOOL_GROUPS');
  const table = src.slice(start, src.indexOf('\n];', start));
  const by = new Map(styles.map((s) => [s.key, s]));
  const words = new Set(['text', 'callout']);
  return [...table.matchAll(/\(\s*"([^"]+)",\s*&\[([^\]]*)\]\s*,?\s*\)/g)].map((m) => ({
    name: m[1],
    tools: [...m[2].matchAll(/"([^"]+)"/g)].map((k) => k[1]).filter((k) => !words.has(k) && by.has(k)).map((k) => by.get(k)!),
  }));
}

/** The plot scales from cave-sheet's `scales()`: `imperial("1\" = 20 ft", 20.0)` and `metric("1:500", 500.0)`. */
function scales() {
  const src = read('crates/cave-sheet/src/lib.rs');
  const start = src.indexOf('pub fn scales()');
  const body = src.slice(start, src.indexOf('\n}', start));
  const out: { label: string; feet_per_inch: number; metric: boolean }[] = [];
  for (const m of body.matchAll(/(imperial|metric)\("((?:[^"\\]|\\.)*)",\s*([\d.]+)\)/g)) {
    const n = Number(m[3]);
    out.push({ label: m[2].replace(/\\"/g, '"'), feet_per_inch: m[1] === 'metric' ? n / 12 : n, metric: m[1] === 'metric' });
  }
  return out;
}

/** Replaces the wasm engine with the mock. Every name engine.ts imports from it is exported, so it can never go stale: the ones the mock does not know answer with an empty list. */
function mockWasm(): Plugin {
  return {
    name: 'mock-cave-wasm',
    enforce: 'pre',
    resolveId(id) {
      if (/\/wasm\/cave_wasm\.js$/.test(id)) return VIRTUAL;
    },
    load(id) {
      if (id !== VIRTUAL) return;
      const engine = read('apps/web/src/lib/engine.ts');
      const m = /import init, \{([^}]*)\} from '\.\.\/wasm\/cave_wasm\.js';/.exec(engine);
      const names = (m?.[1] ?? '').split(',').map((n) => n.trim()).filter(Boolean);
      const styles = palette();
      const groups = toolGroups(styles);
      const generated = {
        palette: styles,
        draw_tool_groups: groups,
        draw_tools: groups.flatMap((g) => g.tools),
        sheet_options: { papers: [], scales: scales() },
      };
      const impl = JSON.stringify(path.resolve(here, 'mock-impl.js'));
      return [
        `import { impl } from ${impl};`,
        `const GEN = ${JSON.stringify(generated)};`,
        'export default async function init() {}',
        ...names.map((n) => `export const ${n} = (...a) => (impl.${n} ? impl.${n}(...a) : GEN.${n} !== undefined ? ${'JSON.stringify(GEN.' + n + ')'} : '[]');`),
      ].join('\n');
    },
  };
}

export default defineConfig({
  root: here,
  plugins: [mockWasm(), svelte()],
  server: { host: true, port: 5199, strictPort: true, fs: { allow: [root] } },
});
