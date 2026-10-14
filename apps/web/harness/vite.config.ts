// A way to touch the Survey panel on any machine, with no Rust build: the real panel and the real session
// code, with the engine replaced by a small in-memory mock of the notebook (harness/mock-impl.js). Run
// `npm run harness` in apps/web and open http://localhost:5199 (or the Network address on a tablet on the
// same network). What it cannot show: the Map, Profile and Cross Section (they need the real engine) and
// Bluetooth (use the Simulator; the browser's chooser needs a real X1).
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, type Plugin } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

const here = path.dirname(fileURLToPath(import.meta.url));
const VIRTUAL = '\0cave-wasm-mock';

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
      const engine = fs.readFileSync(path.resolve(here, '../src/lib/engine.ts'), 'utf8');
      const m = /import init, \{([^}]*)\} from '\.\.\/wasm\/cave_wasm\.js';/.exec(engine);
      const names = (m?.[1] ?? '').split(',').map((n) => n.trim()).filter(Boolean);
      const impl = JSON.stringify(path.resolve(here, 'mock-impl.js'));
      return [
        `import { impl } from ${impl};`,
        'export default async function init() {}',
        ...names.map((n) => `export const ${n} = (...a) => (impl.${n} ? impl.${n}(...a) : '[]');`),
      ].join('\n');
    },
  };
}

export default defineConfig({
  root: here,
  plugins: [mockWasm(), svelte()],
  server: { host: true, port: 5199, strictPort: true, fs: { allow: [path.resolve(here, '../../..')] } },
});
