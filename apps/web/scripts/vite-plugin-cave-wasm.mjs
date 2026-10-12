// Builds crates/cave-wasm with wasm-pack and keeps it fresh while you develop.
//
// Goal: after you edit Rust, a plain browser refresh shows the new code, never a stale copy.
//  - Watches crates/**/*.rs, Cargo.toml and crates/*/data/*.json (data compiled into the engine), rebuilds, then reloads open pages.
//  - While a rebuild is running, every request waits for it, so a refresh mid-build still gets
//    the NEW wasm (never the old one).
//  - Serves /build-info.json (build time, git hash, last error) so the page can say what it runs.
import { spawn, execFileSync } from 'node:child_process';
import { watch } from 'node:fs';
import path from 'node:path';

const repo = path.resolve(import.meta.dirname, '../../..');
const outDir = path.resolve(import.meta.dirname, '../src/wasm');
// Where rustup's tools usually are on a developer's machine, put first; the system's own PATH follows,
// joined with the separator of the system this runs on (a colon on macOS and Linux, a semicolon on Windows).
const home = process.env.HOME ?? process.env.USERPROFILE ?? '';
const env = {
  ...process.env,
  PATH: ['/opt/homebrew/opt/rustup/bin', path.join(home, '.cargo', 'bin'), process.env.PATH ?? ''].join(path.delimiter),
};

function git(...args) {
  try {
    return execFileSync('git', args, { cwd: repo, encoding: 'utf8' }).trim();
  } catch {
    return '';
  }
}

function describeTree() {
  return { git: git('rev-parse', '--short', 'HEAD') || 'unknown', dirty: git('status', '--porcelain') !== '' };
}

function buildWasm(release) {
  return new Promise((resolve) => {
    const args = ['build', 'crates/cave-wasm', '--target', 'web', '--out-dir', outDir, '--out-name', 'cave_wasm', release ? '--release' : '--dev'];
    const p = spawn('wasm-pack', args, { cwd: repo, env });
    let log = '';
    p.stdout.on('data', (d) => (log += d));
    p.stderr.on('data', (d) => (log += d));
    p.on('error', (e) => resolve({ ok: false, log: String(e) }));
    p.on('close', (code) => resolve({ ok: code === 0, log }));
  });
}

function lastError(log) {
  const lines = log.split('\n').filter((l) => /error/i.test(l));
  return (lines.slice(0, 6).join('\n') || log.slice(-600)).trim();
}

export default function caveWasm() {
  let isBuild = false;
  const info = { builtAt: null, ...describeTree(), error: null };

  async function run(release) {
    let r = await buildWasm(release);
    // A build that fails once is often a blip (two cargo runs touching the same output); only a
    // failure that repeats is worth showing.
    if (!r.ok) {
      await new Promise((done) => setTimeout(done, 600));
      r = await buildWasm(release);
    }
    Object.assign(info, describeTree(), { builtAt: new Date().toISOString(), error: r.ok ? null : lastError(r.log) });
    if (!r.ok) console.error(`\n[cave-wasm] build failed:\n${info.error}\n`);
    else console.log(`[cave-wasm] built ${info.git}${info.dirty ? '+' : ''} at ${info.builtAt}`);
    return r;
  }

  return {
    name: 'cave-wasm',
    configResolved(c) {
      isBuild = c.command === 'build';
    },
    async buildStart() {
      if (!isBuild) return;
      const r = await run(true);
      if (!r.ok) throw new Error(`wasm build failed:\n${info.error}`);
    },
    generateBundle() {
      if (isBuild) this.emitFile({ type: 'asset', fileName: 'build-info.json', source: JSON.stringify(info) });
    },
    configureServer(server) {
      let pending = run(false);
      let timer = null;
      let building = false;
      let again = false;
      let release = () => {};

      const settle = async () => {
        building = true;
        do {
          again = false;
          await run(false);
        } while (again);
        building = false;
        release();
        server.ws.send({ type: 'full-reload' });
      };

      watch(path.join(repo, 'crates'), { recursive: true }, (_e, file) => {
        if (!file || file.includes('target') || !(file.endsWith('.rs') || file.endsWith('Cargo.toml') || /[\\/]data[\\/].*\.json$/.test(file))) return;
        if (!building && !timer) pending = new Promise((r) => (release = r));
        clearTimeout(timer);
        timer = setTimeout(() => {
          timer = null;
          if (building) again = true;
          else settle();
        }, 200);
      });

      server.middlewares.use(async (req, res, next) => {
        await pending;
        if (req.url?.split('?')[0] === '/build-info.json') {
          res.setHeader('Cache-Control', 'no-store');
          res.setHeader('Content-Type', 'application/json');
          res.end(JSON.stringify(info));
          return;
        }
        next();
      });
    },
  };
}
