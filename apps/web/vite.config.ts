import { defineConfig, loadEnv, type UserConfig } from 'vite';
import path from 'node:path';
import { svelte } from '@sveltejs/vite-plugin-svelte';
// @ts-expect-error plain JS plugin without types
import caveWasm from './scripts/vite-plugin-cave-wasm.mjs';
// @ts-expect-error plain JS plugin without types
import caveSync from './scripts/vite-plugin-cave-sync.mjs';

// Dev and preview never cache: a plain refresh always loads the latest code.
const noStore = { 'Cache-Control': 'no-store' };

const config: UserConfig = {
  plugins: [caveWasm(), caveSync(), svelte()],
  // The demo caves live with the Rust tests, outside this folder.
  // host: true so a tablet or second laptop on the same network can open this one's address.
  server: { host: true, port: 5173, strictPort: true, headers: noStore, fs: { allow: [path.resolve(__dirname, '../..')] }, proxy: { '/api': `http://localhost:${process.env.CAVECAD_API_PORT ?? '8787'}` } },
  preview: { host: true, port: 4173, strictPort: true, headers: noStore },
  build: { target: 'es2022' },
};

// A build that people will sign in to needs the offline pass's public key and TinyAuth's sign-out address;
// without them offline use or Sign out silently break, so the build stops instead.
const NEEDED_FOR_BUILD = ['VITE_PASS_PUBLIC_KEY', 'VITE_SIGNOUT_URL'];

export default defineConfig(({ command, mode }) => {
  if (command === 'build') {
    const env = loadEnv(mode, __dirname, 'VITE_');
    const missing = NEEDED_FOR_BUILD.filter((k) => !env[k]?.trim());
    if (missing.length) throw new Error(`CaveCAD build: ${missing.join(' and ')} ${missing.length > 1 ? 'are' : 'is'} not set. Put ${missing.length > 1 ? 'them' : 'it'} in apps/web/.env.local or the build environment (see deploy/README.md).`);
  }
  return config;
});
