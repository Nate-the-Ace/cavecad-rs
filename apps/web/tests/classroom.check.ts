import { readFileSync } from 'node:fs';
import { READS, WRITES, guardEngine, readOnly } from '../src/lib/readonly.ts';
import { applyMirror, captureMirror, fitRect, forgetMirror, frontPanels, layoutShape, registerMirror, roundRect, visibleRect } from '../src/lib/classroom-mirror.ts';
import { classLink, clock, mergeEntries, readClassLink, transcript, transcriptName, waiting, type ChatEntry } from '../src/lib/classroom-log.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };

// ---- every engine call is a read or a write, and a write is refused for a student ---------------
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
ok(fake.addMapLine() === 'ok' && fake.undo() === true && calls.length === 2, 'a person who is not watching can write');
calls.length = 0;
readOnly.on = true;
ok(String(fake.addMapLine()).startsWith('error: '), 'a write is refused with an error, as the callers expect');
ok(fake.undo() === false, 'undo says nothing was undone');
ok((fake.openDxf() as { ok: boolean }).ok === false, 'opening a file says it did not open');
ok(calls.length === 0, 'and none of them reached the engine');
fake.scene();
fake.syncApply();
ok(calls.join() === 'scene,syncApply', 'reads, and the teacher\'s edits arriving, still work');
readOnly.on = false;

// ---- the transcript ------------------------------------------------------------------------------
const at = new Date(2026, 9, 13, 14, 5).getTime();
const e = (n: number, extra: Partial<ChatEntry>): ChatEntry => ({ n, at: at + n * 60_000, kind: 'say', text: '', ...extra });
const entries = [
  e(1, { kind: 'hand', name: 'Sam', text: 'raised a hand' }),
  e(2, { name: 'Sam', role: 'student', text: 'Why does the loop close 2% off?' }),
  e(3, { name: 'Dr. Lee', role: 'teacher', text: 'Good question: look at A12.' }),
  e(4, { kind: 'hand', name: 'Sam', text: 'was called on' }),
  e(5, { kind: 'system', text: 'The class has ended.' }),
];
const t = transcript('Caving 101', at, entries, 'Dr. Lee').split('\n');
ok(t[0] === 'Class: Caving 101' && t[1] === 'Teacher: Dr. Lee' && t[2] === 'Started: 2026-10-13 14:05', `header: ${t.slice(0, 3)}`);
ok(t.includes('14:07  Sam: Why does the loop close 2% off?'), 'a question is a line with the time and the name');
ok(t.includes('14:08  Dr. Lee (teacher): Good question: look at A12.'), 'the teacher is marked');
ok(t.includes('14:06  * Sam raised a hand') && t.includes('14:10  -- The class has ended.'), 'hands and notices are marked');
ok(clock(at) === '14:05', 'clock');
ok(waiting(1000, 31_000) === '30 s' && waiting(0, 150_000) === '2 min', 'waiting');
ok(transcriptName('Caving 101: Truitt!', at) === 'Caving-101-Truitt-2026-10-13.txt' && transcriptName('', at) === 'class-2026-10-13.txt', 'a safe file name');
ok(mergeEntries(entries, [entries[1], e(6, { text: 'x' })]).map((x) => x.n).join() === '1,2,3,4,5,6', 'a repeated backlog adds nothing');
ok(mergeEntries([], [entries[2], entries[0]]).map((x) => x.n).join() === '1,3', 'entries come out in order');

// ---- links ---------------------------------------------------------------------------------------
const link = classLink('http://10.0.0.5:8080/app/?x=1#top', 'Caving 101', 'abc123');
ok(link === 'http://10.0.0.5:8080/app/?class=Caving%20101&invite=abc123', link);
const back = readClassLink(new URL(link).search);
ok(back?.room === 'Caving 101' && back.invite === 'abc123', 'a link reads back');
ok(readClassLink('?join=room&invite=x') === null && readClassLink('') === null, 'other links are not class links');

// ---- the teacher's view --------------------------------------------------------------------------
const rect = { x0: -10, y0: -5, x1: 30, y1: 15 };
const f = fitRect(rect, 800, 400);
const r2 = visibleRect(f.scale, f.ox, f.oy, 800, 400);
ok(Math.abs(r2.x0 - -10) < 1e-9 && Math.abs(r2.x1 - 30) < 1e-9 && Math.abs(r2.y0 - -5) < 1e-9 && Math.abs(r2.y1 - 15) < 1e-9, 'a view fitted into a window of the same shape shows the same rectangle');
const narrow = visibleRect(...Object.values(fitRect(rect, 200, 400)) as [number, number, number], 200, 400);
ok(narrow.x0 <= -10 && narrow.x1 >= 30 && narrow.y0 <= -5 && narrow.y1 >= 15, 'a window of another shape shows at least that rectangle');
const c = visibleRect(f.scale, f.ox, f.oy, 800, 400);
ok(Math.abs((c.x0 + c.x1) / 2 - 10) < 1e-9 && Math.abs((c.y0 + c.y1) / 2 - 5) < 1e-9, 'and keeps its centre');
ok(roundRect({ x0: 1.23456, y0: 2, x1: 3.999, y1: 4 }).x0 === 1.23, 'rounded to centimetres');

const layoutA = { grid: { root: { type: 'branch', size: 900, data: [{ type: 'leaf', size: 300, data: { views: ['map', 'profile'], activeView: 'map', id: '1' } }, { type: 'leaf', size: 600, data: { views: ['stations'], activeView: 'stations', id: '2' } }] }, width: 1200, height: 800 }, activeGroup: '1' };
const layoutSized = JSON.parse(JSON.stringify(layoutA).replace('"size":300', '"size":450').replace('"width":1200', '"width":1500').replace('"activeView":"map"', '"activeView":"profile"'));
const layoutMoved = JSON.parse(JSON.stringify(layoutA).replace('["map","profile"]', '["profile","map"]'));
ok(layoutShape(layoutA) === layoutShape(layoutSized), 'resizing a splitter or changing tab is not a new arrangement');
ok(layoutShape(layoutA) !== layoutShape(layoutMoved), 'moving a panel is');
ok(frontPanels(layoutA).join() === 'map,stations' && frontPanels(layoutSized).join() === 'profile,stations', 'the panel in front of each group');

forgetMirror();
let a = '', b = '';
const offA = registerMirror('a', { read: () => ({ v: 1 }), write: (s) => (a = JSON.stringify(s)) });
registerMirror('gone', { read: () => undefined, write: () => undefined });
registerMirror('broken', { read: () => { throw new Error('not ready'); }, write: () => { throw new Error('cannot'); } });
ok(JSON.stringify(captureMirror()) === '{"a":{"v":1}}', 'what the teacher sends: parts that read, and only those');
applyMirror({ a: { v: 2 }, later: { w: 3 }, broken: 1 });
ok(a === '{"v":2}', 'a part is given its state (and one that cannot take it does not stop the others)');
registerMirror('later', { read: () => ({ w: 0 }), write: (s) => (b = JSON.stringify(s)) });
await Promise.resolve();
ok(b === '{"w":3}', 'a part that registers after the state arrived is given it at once');
offA();
applyMirror({ a: { v: 9 } });
ok(a === '{"v":2}', 'a part that is gone is not written to');

readOnly.on = false;
if (bad) process.exit(1);
console.log('classroom ok');
