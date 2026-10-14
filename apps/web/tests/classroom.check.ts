import { readFileSync } from 'node:fs';
import { READS, WRITES, guardEngine, readOnly } from '../src/lib/readonly.ts';
import { applyMirror, captureMirror, fitRect, forgetMirror, frontPanels, layoutShape, registerMirror, roundRect, visibleRect } from '../src/lib/classroom-mirror.ts';
import { CLASS_MESSAGES, classJoined, classLeft, classMessage, classWelcome, newClass } from '../src/lib/classroom-room.mjs';
import { classLink, liveLink, readLiveLink, clock, mergeEntries, readClassLink, transcript, transcriptName, waiting, type ChatEntry } from '../src/lib/classroom-log.ts';

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

// ---- the direct class link ---------------------------------------------------------------------
{
  const plain = liveLink('http://10.0.0.5:8080/app/?x=1#top', 'sess123abc');
  ok(plain === 'http://10.0.0.5:8080/app/?live=sess123abc', plain);
  const r = readLiveLink(new URL(plain).search);
  ok(r?.session === 'sess123abc' && r.mailbox === null, 'a link reads back, with no mailbox of its own');
  const own = liveLink('https://x.example/app/', 'sess123abc', { endpoint: 'https://script.google.com/macros/s/ID/exec', key: 'k&1' });
  const r2 = readLiveLink(new URL(own).search);
  ok(r2?.mailbox?.endpoint === 'https://script.google.com/macros/s/ID/exec' && r2.mailbox.key === 'k&1', 'a mailbox set on the teacher machine travels with the link');
  ok(readLiveLink('?class=a&invite=b') === null && readLiveLink('') === null, 'other links are not direct class links');
}

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


// ---- the rules of a class, with no network (the same code runs on the relay and in a peer session) ----
{
  const sent: { to: string; m: Record<string, any> }[] = [];
  const people = [
    { id: 't', name: 'Dr. Lee', role: 'teacher' },
    { id: 's1', name: 'Sam', role: 'student' },
    { id: 's2', name: 'Kim', role: 'student' },
  ];
  const ioFor = (from: string) => ({
    members: () => people,
    send: (id: string, m: object) => sent.push({ to: id, m: m as Record<string, any> }),
    all: (m: object) => people.forEach((p) => sent.push({ to: p.id, m: m as Record<string, any> })),
    others: (m: object) => people.filter((p) => p.id !== from).forEach((p) => sent.push({ to: p.id, m: m as Record<string, any> })),
  });
  const recorded: object[] = [];
  const cls = newClass('Caving 101', (e) => recorded.push(e));
  const [t, s1, s2] = people;
  const to = (id: string, kind: string) => sent.filter((x) => x.to === id && x.m.t === kind);
  ok(CLASS_MESSAGES.join() === 'say,hand,mirror,end,getlog', 'the messages that belong to a class');

  classMessage(cls, s1, { t: 'say', text: '  Why is   it\n off?  ' }, ioFor('s1'));
  ok(to('t', 'chat')[0].m.entry.text === 'Why is it off?' && to('s1', 'chat').length === 1 && recorded.length === 1, 'a line is tidied, goes to everyone (the sender too) and is recorded');
  sent.length = 0;
  classMessage(cls, s1, { t: 'say', text: 'again' }, ioFor('s1'));
  ok(to('s1', 'denied').length === 1 && to('t', 'chat').length === 0, 'a student cannot say two things at once');
  classMessage(cls, s1, { t: 'mirror', state: { tab: 'Draw' } }, ioFor('s1'));
  ok(to('s1', 'denied')[1].m.what === 'view' && cls.mirror === '', 'a student cannot steer the view');
  sent.length = 0;
  classMessage(cls, t, { t: 'mirror', state: { tab: 'Draw' } }, ioFor('t'));
  ok(to('s1', 'mirror').length === 1 && to('t', 'mirror').length === 0 && cls.mirror === '{"tab":"Draw"}', 'the view of the teacher goes to the students, kept for latecomers');
  ok(classWelcome(cls, 'student').mirror === '{"tab":"Draw"}' && classWelcome(cls, 'student').chat.length === 1, 'a latecomer is given the view and the chat');

  sent.length = 0;
  classMessage(cls, s1, { t: 'hand', up: true }, ioFor('s1'));
  classMessage(cls, s2, { t: 'hand', up: false, who: 's1' }, ioFor('s2'));
  ok([...cls.hands.keys()].join() === 's1', 'a student can only change their own hand (a who is ignored)');
  classMessage(cls, s2, { t: 'hand', up: true, who: 's1' }, ioFor('s2'));
  ok([...cls.hands.keys()].sort().join() === 's1,s2', 'so asking for another student puts up their own');
  ok(to('t', 'hands').at(-1)!.m.hands.length === 2, 'and the teacher is told');
  classMessage(cls, t, { t: 'hand', up: false, who: 's1' }, ioFor('t'));
  ok([...cls.hands.keys()].join() === 's2' && cls.chat.at(-1)?.text === 'was called on', 'the teacher calls on a student');
  classLeft(cls, 's2', ioFor('s2'));
  ok(cls.hands.size === 0, 'a student who leaves puts their hand down');
  sent.length = 0;
  classMessage(cls, s1, { t: 'getlog' }, ioFor('s1'));
  ok(sent.length === 0, 'students cannot ask for the log');
  classMessage(cls, t, { t: 'getlog' }, ioFor('t'));
  ok(to('t', 'log')[0].m.entries.length === cls.chat.length && cls.chat.every((e, i) => e.n === i + 1), 'the teacher can, in order');
  classMessage(cls, t, { t: 'end' }, ioFor('t'));
  sent.length = 0;
  classMessage(cls, s1, { t: 'say', text: 'hello?' }, ioFor('s1'));
  ok(to('s1', 'denied')[0].m.message === 'The class has ended.', 'nothing is said after the end');
  ok(recorded.length === cls.chat.length, 'every line of the record was handed to the recorder');
}

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

// who comes and goes is in the record, for everyone
{
  const room = newClass('Joins');
  const sent: { to: string; m: any }[] = [];
  const members = [{ id: 't', name: 'Tess', role: 'teacher' }, { id: 's1', name: 'Bob', role: 'student' }];
  const io = { members: () => members, send: (to: string, m: object) => void sent.push({ to, m }), all: (m: object) => void sent.push({ to: '*', m }), others: (m: object) => void sent.push({ to: '!', m }) };
  classJoined(room, { id: 's1', name: 'Bob', role: 'student' }, io);
  ok(room.chat.length === 1 && room.chat[0].kind === 'system' && room.chat[0].name === 'Bob' && room.chat[0].text === 'joined the class', 'a join is a line in the record');
  ok(sent.some((x) => x.to === '*' && x.m.t === 'chat' && x.m.entry.text === 'joined the class'), 'and goes to everyone');
  classMessage(room, { id: 's1', name: 'Bob', role: 'student' }, { t: 'hand', up: true }, io);
  classLeft(room, 's1', io, { name: 'Bob', role: 'student' });
  const last = room.chat[room.chat.length - 1];
  ok(last.kind === 'system' && last.text === 'left the class' && last.name === 'Bob', 'a departure is a line in the record');
  ok(room.hands.size === 0, 'and the hand goes down');
  const text = transcript('Joins', room.started, room.chat as ChatEntry[], 'Tess');
  ok(/-- Bob joined the class/.test(text) && /-- Bob left the class/.test(text), 'the transcript says who came and went, by name');
  room.ended = true;
  classJoined(room, { id: 's2', name: 'Late', role: 'student' }, io);
  ok(!room.chat.some((e) => e.name === 'Late'), 'nothing is recorded after the class has ended');
}

readOnly.on = false;
if (bad) process.exit(1);
console.log('classroom ok');
