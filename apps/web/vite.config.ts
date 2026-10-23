import { defineConfig } from 'vite';
import path from 'node:path';
import { svelte } from '@sveltejs/vite-plugin-svelte';
// @ts-expect-error plain JS plugin without types
import caveWasm from './scripts/vite-plugin-cave-wasm.mjs';
// @ts-expect-error plain JS plugin without types
import caveSync from './scripts/vite-plugin-cave-sync.mjs';

// Dev and preview never cache: a plain refresh always loads the latest code.
const noStore = { 'Cache-Control': 'no-store' };

export default defineConfig({
  plugins: [caveWasm(), caveSync(), svelte()],
  // The demo caves live with the Rust tests, outside this folder.
  // host: true so a tablet or second laptop on the same network can open this one's address.
  server: { host: true, port: 5173, strictPort: true, headers: noStore, fs: { allow: [path.resolve(__dirname, '../..')] }, proxy: { '/api': 'http://localhost:8787' } },
  preview: { host: true, port: 4173, strictPort: true, headers: noStore },
  build: { target: 'es2022' },
});
