import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
// @ts-expect-error plain JS plugin without types
import caveWasm from './scripts/vite-plugin-cave-wasm.mjs';

// Dev and preview never cache: a plain refresh always loads the latest code.
const noStore = { 'Cache-Control': 'no-store' };

export default defineConfig({
  plugins: [caveWasm(), svelte()],
  server: { port: 5173, strictPort: true, headers: noStore },
  preview: { port: 4173, strictPort: true, headers: noStore },
  build: { target: 'es2022' },
});
