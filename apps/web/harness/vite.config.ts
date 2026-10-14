// A way to touch the Survey panel and the sketchpad: the real ribbon, panels, session and engine, with a fresh empty
// cave opened in the browser. Run `npm run harness` in apps/web and open http://localhost:5199 (or the Network
// address on a tablet on the same network). The engine is the real one (the Rust is built the way the app builds it);
// what the harness replaces is only the handful of calls that would reach the app's own drawn lines, which in the
// sketchpad are the sketchpad's strokes (`overrides.ts`). What it cannot show: the Map, and Bluetooth (use the Simulator).
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, type Plugin } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
// @ts-expect-error plain JS plugin without types
import caveWasm from '../scripts/vite-plugin-cave-wasm.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const root = path.resolve(here, '../../..');
const VIRTUAL = '\0cave-wasm-harness';
const REAL = path.resolve(here, '../src/wasm/cave_wasm.js');

/**
 * Wraps the engine: every name `engine.ts` imports from the wasm module is re-exported, and the ones the
 * harness has its own answer for (`overrides.ts`) go there first, falling back to the real engine (`main.ts` puts them on `globalThis`: importing them here would loop back into the engine). Reading the
 * names from `engine.ts` means the wrapper can never go stale.
 */
function harnessEngine(): Plugin {
  return {
    name: 'harness-engine',
    enforce: 'pre',
    resolveId(id) {
      if (/\/wasm\/cave_wasm\.js$/.test(id)) return VIRTUAL;
    },
    async load(id) {
      if (id !== VIRTUAL) return;
      const { readFileSync } = await import('node:fs');
      const engine = readFileSync(path.resolve(here, '../src/lib/engine.ts'), 'utf8');
      const m = /import init, \{([^}]*)\} from '\.\.\/wasm\/cave_wasm\.js';/.exec(engine);
      const names = (m?.[1] ?? '').split(',').map((n) => n.trim()).filter(Boolean);
      return [
        `import init, * as real from ${JSON.stringify(REAL + '?real')};`,
        'export default init;',
        ...names.map((n) => `export const ${n} = (...a) => { const o = globalThis.__harnessOverrides?.${n}; if (o) { const r = o(real.${n}, ...a); if (r !== undefined) return r; } return real.${n}(...a); };`),
      ].join('\n');
    },
  };
}

export default defineConfig({
  root: here,
  plugins: [caveWasm(), harnessEngine(), svelte()],
  server: { host: true, port: 5199, strictPort: true, fs: { allow: [root] } },
});
