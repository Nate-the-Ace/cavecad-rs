import { readFileSync } from 'node:fs';
import { READS, REPLACES, WRITES, guardEngine, readOnly, setWatchLock } from '../src/lib/readonly.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };

// ---- every engine call is a read or a write, and a write is refused while locked ----------------
const src = readFileSync(new URL('../src/lib/engine.ts', import.meta.url), 'utf8');
const body = src.slice(src.indexOf('const rawEngine = {'), src.indexOf('export const engine = guardEngine'));
const methods = [...body.matchAll(/^ {2}([a-zA-Z]+): /gm)].map((m) => m[1]);
ok(methods.length > 50, `found the engine's methods (${methods.length})`);
for (const name of methods) {
  const r = READS.includes(name), w = WRITES.includes(name);
  ok(r !== w, `engine.${name} must be listed as a read or as a write in lib/readonly.ts (reads: ${r}, writes: ${w})`);
}
for (const name of [...READS, ...WRITES]) ok(methods.includes(name), `readonly.ts lists ${name}, which engine.ts does not have`);

const calls: string[] = [];
const fake = guardEngine({
  addMapLine: () => (calls.push('addMapLine'), 'ok'),
  undo: () => (calls.push('undo'), true),
  openDxf: () => (calls.push('openDxf'), { ok: true, message: 'opened', warnings: [] }),
  scene: () => (calls.push('scene'), { stations: [] }),
  syncApply: () => (calls.push('syncApply'), 1),
});
readOnly.on = false;
ok(fake.addMapLine() === 'ok' && fake.undo() === true && calls.length === 2, 'an unlocked cave can be written');
calls.length = 0;
readOnly.on = true;
ok(String(fake.addMapLine()).startsWith('error: '), 'a write is refused with an error, as the callers expect');
ok(fake.undo() === false, 'undo says nothing was undone');
ok((fake.openDxf() as { ok: boolean }).ok === false, 'opening a file says it did not open');
ok(calls.length === 0, 'and none of them reached the engine');
fake.scene();
fake.syncApply();
ok(calls.join() === 'scene,syncApply', 'reads, and edits arriving from the server, still work');
readOnly.on = false;

// ---- a watcher's lock refuses changes to the cave, not opening another --------------------------------
for (const name of REPLACES) ok(WRITES.includes(name), `${name} (a cave-replacing call) is a write`);
calls.length = 0;
setWatchLock(true);
ok(String(fake.addMapLine()).startsWith('error: '), 'a watcher cannot draw');
ok(fake.undo() === false, 'a watcher cannot undo');
ok((fake.openDxf() as { ok: boolean }).ok === true, 'a watcher can still open another file');
fake.syncApply();
ok(calls.join() === 'openDxf,syncApply', 'and edits from the room still arrive');
setWatchLock(false);
calls.length = 0;
ok(fake.addMapLine() === 'ok' && calls.join() === 'addMapLine', 'the lock comes off');

if (bad) process.exit(1);
console.log('readonly ok');
