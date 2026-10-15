// Lets Node's test runner load the app's source the way Vite does: a relative import with no extension (`./scan-warp`,
// `./line-style.svelte`) is tried as `.ts`, `.svelte.ts` and `/index.ts`. A module that uses a Svelte rune (`$state`) at its
// top level gets a plain stand-in (the value itself), which is all a test of its pure parts needs.
// Use: `node --experimental-strip-types --import ./tests/ts-resolve.mjs tests/x.check.ts`.
import { registerHooks } from 'node:module';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

globalThis.$state = (v) => v;
globalThis.$derived = (v) => v;

registerHooks({
  resolve(specifier, context, next) {
    try {
      return next(specifier, context);
    } catch (e) {
      if (!/^\.\.?\//.test(specifier) || !context.parentURL?.startsWith('file:')) throw e;
      for (const tail of ['.ts', '.svelte.ts', '/index.ts']) {
        const url = new URL(specifier + tail, context.parentURL);
        if (existsSync(fileURLToPath(url))) return next(url.href, context);
      }
      throw e;
    }
  },
});
