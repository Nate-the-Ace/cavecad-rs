// The project folder in a browser (lib/folder-handle.ts) against an in-memory directory handle, with
// the real rules from the engine (cave-folder through WASM): New cave makes every subfolder, finds a
// cave or a folder already there, writes the cave file, backs up the last copy at most once in ten
// minutes, thins old backups and never touches anything else.
// Run: npm run test:newcave (build the WASM first: npm run dev does it).
import { readFileSync } from 'node:fs';
import { initSync, folder_plan } from '../src/wasm/cave_wasm.js';
import { lookIn, makeIn, readIn, writeIn, exportsIn, namesIn, type Dir, type FileH, type Plan } from '../src/lib/folder-handle.ts';

initSync({ module: readFileSync(new URL('../src/wasm/cave_wasm_bg.wasm', import.meta.url)) });
const plan: Plan = (inside, backups, stem, now) => JSON.parse(folder_plan(JSON.stringify(inside), JSON.stringify(backups), stem, now));

let failures = 0;
function check(what: string, ok: boolean, detail = '') {
  if (!ok) failures += 1;
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}${ok || !detail ? '' : `: ${detail}`}`);
}

const missing = (name: string) => Object.assign(new DOMException(`${name} not found`, 'NotFoundError'));

/** A directory in memory, shaped like the browser's handle. */
function dir(name: string): Dir & { files: Map<string, Uint8Array>; dirs: Map<string, ReturnType<typeof dir>> } {
  const files = new Map<string, Uint8Array>();
  const dirs = new Map<string, ReturnType<typeof dir>>();
  const fileHandle = (n: string): FileH => ({
    kind: 'file',
    name: n,
    getFile: async () => new Blob([files.get(n)! as BlobPart]),
    createWritable: async () => {
      let pending = new Uint8Array();
      return { write: async (d: Uint8Array) => void (pending = d), close: async () => void files.set(n, pending) };
    },
  });
  const self = {
    kind: 'directory' as const,
    name,
    files,
    dirs,
    async *values() {
      for (const n of dirs.keys()) yield { kind: 'directory' as const, name: n };
      for (const n of files.keys()) yield { kind: 'file' as const, name: n };
    },
    async getDirectoryHandle(n: string, o?: { create?: boolean }) {
      if (!dirs.has(n)) {
        if (!o?.create) throw missing(n);
        dirs.set(n, dir(n));
      }
      return dirs.get(n)!;
    },
    async getFileHandle(n: string, o?: { create?: boolean }) {
      if (!files.has(n)) {
        if (!o?.create) throw missing(n);
        files.set(n, new Uint8Array());
      }
      return fileHandle(n);
    },
    async removeEntry(n: string) {
      if (!files.delete(n) && !dirs.delete(n)) throw missing(n);
    },
  };
  return self;
}

const bytes = (t: string) => new TextEncoder().encode(t);
const text = (b: Uint8Array) => new TextDecoder().decode(b);
const SUBS = ['scans', 'survey', 'images', 'lidar', 'surface', 'documents', 'trips', 'exports', 'backup', '.cavecad'];

// a new cave in an empty place
const docs = dir('Documents');
check('nothing there yet', (await lookIn(docs, 'Truitt Cave', plan)).kind === 'empty');
const cave = await makeIn(docs, 'Truitt Cave', plan);
check('every subfolder and the hidden cache are made', JSON.stringify(await namesIn(cave)) === JSON.stringify([...SUBS].sort()), String(await namesIn(cave)));
check('an empty folder is not a cave', (await lookIn(docs, 'Truitt Cave', plan)).kind === 'notACave');

// saving: the first write has nothing to back up; later ones back up once in ten minutes
const file = 'Truitt Cave.cavecad';
await writeIn(cave, file, bytes('one'), '2026-10-26 1400', plan);
const backup = cave.dirs.get('backup')!;
check('the cave file is written', text(await readIn(cave, file)) === 'one');
check('no backup of nothing', backup.files.size === 0);
await writeIn(cave, file, bytes('two'), '2026-10-26 1401', plan);
await writeIn(cave, file, bytes('three'), '2026-10-26 1405', plan);
await writeIn(cave, file, bytes('four'), '2026-10-26 1412', plan);
check('backed up at 1401 and 1412 only', JSON.stringify([...backup.files.keys()].sort()) === JSON.stringify(['Truitt Cave 2026-10-26 1401.cavecad', 'Truitt Cave 2026-10-26 1412.cavecad']), String([...backup.files.keys()]));
check('each backup holds the copy it replaced', text(backup.files.get('Truitt Cave 2026-10-26 1401.cavecad')!) === 'one' && text(backup.files.get('Truitt Cave 2026-10-26 1412.cavecad')!) === 'three');
check('the file holds the latest save', text(await readIn(cave, file)) === 'four');
const look = await lookIn(docs, 'Truitt Cave', plan);
check('a folder with a cave in it is found', look.kind === 'hasCave' && look.file === file, JSON.stringify(look));

// thinning: 35 old backups on one day plus someone's own file
for (let i = 0; i < 35; i++) backup.files.set(`Truitt Cave 2026-10-01 ${String(i % 24).padStart(2, '0')}${i < 24 ? '00' : '30'}.cavecad`, bytes('x'));
backup.files.set('my notes.txt', bytes('mine'));
await writeIn(cave, file, bytes('five'), '2026-10-26 1500', plan);
const left = [...backup.files.keys()];
check('thirty newest backups and the person\'s file stay', left.length === 31 && left.includes('my notes.txt'), String(left.length));

// an old cave's folder: what it has is kept, PDF takes the exports
const old = dir('Old Pit');
old.dirs.set('Scans', dir('Scans'));
old.dirs.set('PDF', dir('PDF'));
old.dirs.get('Scans')!.files.set('p1.png', bytes('page'));
const parent = dir('Caves');
parent.dirs.set('Old Pit', old);
await makeIn(parent, 'Old Pit', plan);
check('an existing Scans/ is not doubled', !old.dirs.has('scans') && text(old.dirs.get('Scans')!.files.get('p1.png')!) === 'page');
check('no exports/ beside an old PDF/', !old.dirs.has('exports') && old.dirs.has('survey'));
check('exports go to the old PDF/', (await exportsIn(old, plan)).name === 'PDF');

// Chrome will not hand over Documents itself, so the picked folder may be the cave's own folder
const own = dir('Lantern Cave');
check('an empty folder picked with the cave\'s name counts as nothing there', (await lookIn(own, 'Lantern Cave', plan)).kind === 'empty');
const same = await makeIn(own, 'Lantern Cave', plan);
check('the picked folder is the cave folder: no folder inside it', same === own && !own.dirs.has('Lantern Cave') && own.dirs.has('scans'));
own.files.set('Lantern Cave.cavecad', bytes('cave'));
check('a picked cave folder holding a cave is found', (await lookIn(own, 'Lantern Cave', plan)).kind === 'hasCave');
check('another name still makes a folder inside', (await lookIn(dir('Caves'), 'Lantern Cave', plan)).kind === 'empty');

if (failures) {
  console.log(`${failures} failed`);
  process.exit(1);
}
console.log('all passed');
