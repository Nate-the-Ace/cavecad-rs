// Checks the room relay with real sockets: joining, seeding, forwarding, catching up.
import { createServer } from 'node:http';
import { WebSocket } from 'ws';
import { attachRelay } from '../scripts/sync-relay.mjs';

const http = createServer();
const relay = attachRelay(http);
await new Promise((r) => http.listen(0, r));
const url = `ws://127.0.0.1:${http.address().port}/sync`;

const client = () =>
  new Promise((resolve) => {
    const ws = new WebSocket(url);
    const got = [];
    const waiters = [];
    ws.on('message', (d) => {
      const m = JSON.parse(String(d));
      got.push(m);
      waiters.splice(0).forEach((w) => w());
    });
    const next = async (t) => {
      for (;;) {
        const i = got.findIndex((m) => m.t === t);
        if (i >= 0) return got.splice(i, 1)[0];
        await new Promise((r) => waiters.push(r));
      }
    };
    ws.on('open', () => resolve({ ws, next, send: (m) => ws.send(JSON.stringify(m)) }));
  });
const eq = (a, b, what) => {
  if (JSON.stringify(a) !== JSON.stringify(b)) throw new Error(`${what}: got ${JSON.stringify(a)}, wanted ${JSON.stringify(b)}`);
};

const a = await client();
a.send({ t: 'join', room: 'cave', id: 'a1', name: 'Laptop', color: '#3b82f6' });
const wa = await a.next('welcome');
eq([wa.seed, wa.log.length], [true, 0], 'first device seeds an empty room');

const b = await client();
b.send({ t: 'join', room: 'cave', id: 'b1', name: 'iPad', color: 'red' });
const wb = await b.next('welcome');
eq(wb.seed, false, 'second device is not asked to seed while the first is');
let p = { n: 0 };
while (p.n < 2) p = await a.next('peers');
eq(p.people, [{ id: 'a1', name: 'Laptop', color: '#3b82f6' }, { id: 'b1', name: 'iPad', color: '#888888' }], 'everyone sees who is here (a bad colour is replaced)');

a.send({ t: 'ops', ops: [{ n: 1 }, { n: 2 }] });
eq((await b.next('ops')).ops, [{ n: 1 }, { n: 2 }], 'ops reach the other device');
b.send({ t: 'ops', ops: [{ n: 3 }] });
eq((await a.next('ops')).ops, [{ n: 3 }], 'and back');

// presence: passed on to the others, stamped with the sender's own id, and never kept
a.send({ t: 'presence', id: 'someone-else', view: 'profile', ctx: 'A', x: 3, y: 4, tool: 'wall' });
const pres = await b.next('presence');
eq([pres.id, pres.view, pres.x, pres.y, pres.tool], ['a1', 'profile', 3, 4, 'wall'], 'presence reaches the others under the sender\'s own id');
a.send({ t: 'presence', pad: 'x'.repeat(5000) });
const c = await client();
c.send({ t: 'join', room: 'cave' });
const wc = await c.next('welcome');
eq(wc.log, [{ n: 1 }, { n: 2 }, { n: 3 }], 'a late device catches up on everything, and presence is not part of it');

const other = await client();
other.send({ t: 'join', room: 'elsewhere' });
eq((await other.next('welcome')).seed, true, 'rooms are separate');
eq(relay.rooms.size, 2, 'two rooms');

a.send({ t: 'ops', ops: 'garbage' });
a.ws.send('not json');
a.send({ t: 'ops', ops: [{ n: 4 }] });
eq((await b.next('ops')).ops, [{ n: 4 }], 'bad messages are ignored');

// invites: the first device owns the room; with an invite made, only invite holders get in
const o = await client();
o.send({ t: 'join', room: 'private', id: 'o1', name: 'Owner' });
const wo = await o.next('welcome');
eq([wo.owner, typeof wo.ownerKey], [true, 'string'], 'the first device owns the room and is given its key');
const stranger = await client();
stranger.send({ t: 'join', room: 'private', id: 's1', name: 'Early' });
eq((await stranger.next('welcome')).owner, false, 'while the room is open anyone with the name gets in, and is not the owner');
stranger.send({ t: 'invite', hours: 1 });
const ignored = await Promise.race([stranger.next('invites').then(() => 'answered'), new Promise((r) => setTimeout(() => r('ignored'), 150))]);
eq(ignored, 'ignored', 'only the owner can make invites');
o.send({ t: 'invite', label: 'for Sam', hours: 1, uses: 1 });
const made = await o.next('invites');
eq([made.invites.length, made.invites[0].label, made.invites[0].uses], [1, 'for Sam', 1], 'the owner makes an invite');
const token = made.made;
const nobody = await client();
nobody.send({ t: 'join', room: 'private', id: 'n1' });
eq((await nobody.next('error')).message.includes('invite'), true, 'a room with invites turns away people without one');
const samOld = await client();
samOld.send({ t: 'join', room: 'private', id: 'sam1', name: 'Sam', invite: token });
eq((await samOld.next('welcome')).owner, false, 'an invite lets one person in');
const second = await client();
second.send({ t: 'join', room: 'private', id: 'two1', invite: token });
eq((await second.next('error')).message.includes('already used'), true, 'a one-person invite is used up');
samOld.ws.close();
await new Promise((r) => setTimeout(r, 100));
const samAgain = await client();
samAgain.send({ t: 'join', room: 'private', id: 'sam1', invite: token });
eq((await samAgain.next('welcome')).owner, false, 'the same person coming back does not need a new invite');
o.send({ t: 'invite', hours: 1, uses: 0 });
const open = (await o.next('invites')).made;
o.send({ t: 'revoke', token: open });
const after = await o.next('invites');
eq(after.invites.some((i) => i.token === open), false, 'the owner can take an invite back');
const late = await client();
late.send({ t: 'join', room: 'private', id: 'l1', invite: open });
eq((await late.next('error')).message.includes('expired'), true, 'a taken-back invite no longer works');
o.ws.close();
await new Promise((r) => setTimeout(r, 100));
const owner2 = await client();
owner2.send({ t: 'join', room: 'private', id: 'o1', ownerKey: wo.ownerKey });
eq((await owner2.next('welcome')).owner, true, 'the owner comes back with its key, even into a locked room');


// ---- a classroom: one teacher, students who only watch -------------------------------------------
import { mkdtempSync, readFileSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createRelay } from '../scripts/sync-relay.mjs';
const logDir = mkdtempSync(join(tmpdir(), 'class-'));
const http2 = createServer();
const relay2 = attachRelay(http2, createRelay({ logDir }));
await new Promise((r) => http2.listen(0, r));
const url2 = `ws://127.0.0.1:${http2.address().port}/sync`;
const client2 = () =>
  new Promise((resolve) => {
    const ws = new WebSocket(url2);
    const got = [];
    const waiters = [];
    ws.on('message', (d) => {
      got.push(JSON.parse(String(d)));
      waiters.splice(0).forEach((w) => w());
    });
    const next = async (t) => {
      for (;;) {
        const i = got.findIndex((m) => m.t === t);
        if (i >= 0) return got.splice(i, 1)[0];
        await new Promise((r) => waiters.push(r));
      }
    };
    const quiet = async (t, ms = 150) => await Promise.race([next(t).then(() => false), new Promise((r) => setTimeout(() => r(true), ms))]);
    ws.on('open', () => resolve({ ws, next, quiet, send: (m) => ws.send(JSON.stringify(m)) }));
  });
const pause = (ms) => new Promise((r) => setTimeout(r, ms));

// a student's link never starts a class
const early = await client2();
early.send({ t: 'join', room: 'Caving 101', id: 'e1', name: 'Early', joinOnly: true });
eq((await early.next('error')).message.includes('not started'), true, 'a student cannot start a class by being first');
eq(relay2.rooms.size, 0, 'and no room was made');

const teacher = await client2();
teacher.send({ t: 'join', room: 'Caving 101', id: 't1', name: 'Dr. Lee', color: '#3b82f6', mode: 'class' });
const wt = await teacher.next('welcome');
eq([wt.owner, wt.seed, wt.class.role], [true, true, 'teacher'], 'the first in starts the class as its teacher, and seeds the cave');

// no cave shared yet: nobody can come in to watch nothing
teacher.send({ t: 'invite', label: 'class', hours: 8, uses: 0 });
const link = (await teacher.next('invites')).made;
const tooSoon = await client2();
tooSoon.send({ t: 'join', room: 'Caving 101', id: 'x1', name: 'Quick', invite: link, joinOnly: true });
eq((await tooSoon.next('error')).message.includes('not shared a cave'), true, 'students wait until the teacher has shared the cave');
teacher.send({ t: 'ops', ops: [{ n: 1 }] });
await pause(50);

const sam = await client2();
sam.send({ t: 'join', room: 'Caving 101', id: 's1', name: 'Sam', invite: link, joinOnly: true });
const ws1 = await sam.next('welcome');
eq([ws1.owner, ws1.class.role, ws1.log], [false, 'student', [{ n: 1 }]], 'a student gets the cave and is told they are a student');
const kim = await client2();
kim.send({ t: 'join', room: 'Caving 101', id: 's2', name: 'Kim', invite: link, joinOnly: true });
await kim.next('welcome');
let pe = { people: [] };
while (pe.people.length < 3) pe = await teacher.next('peers');
eq(pe.people.map((p) => p.role), ['teacher', 'student', 'student'], 'the roster says who teaches and who watches');

// students cannot change the cave, or the teacher's view
sam.send({ t: 'ops', ops: [{ n: 99 }] });
eq((await sam.next('denied')).what, 'edit', "a student's edit is refused");
eq(await kim.quiet('ops'), true, 'and does not reach the others');
sam.send({ t: 'mirror', state: { tab: 'Draw' } });
eq((await sam.next('denied')).what, 'view', 'a student cannot steer the view');
teacher.send({ t: 'ops', ops: [{ n: 2 }] });
eq((await sam.next('ops')).ops, [{ n: 2 }], "the teacher's edits reach every student");

// the teacher's view goes to every student, and to a latecomer first
teacher.send({ t: 'mirror', state: { tab: 'Draw', trip: 'T1' } });
eq((await sam.next('mirror')).state, { tab: 'Draw', trip: 'T1' }, "the teacher's view reaches a student");
await kim.next('mirror');
const lena = await client2();
lena.send({ t: 'join', room: 'Caving 101', id: 's3', name: 'Lena', invite: link, joinOnly: true });
eq(JSON.parse((await lena.next('welcome')).class.mirror), { tab: 'Draw', trip: 'T1' }, "a latecomer is given the teacher's current view");

// pointers: students are seen by the teacher, not by each other
sam.send({ t: 'presence', view: 'map', ctx: '', x: 1, y: 2, tool: '' });
eq((await teacher.next('presence')).id, 's1', "the teacher sees a student's pointer");
eq(await kim.quiet('presence'), true, 'other students do not');
teacher.send({ t: 'presence', view: 'map', ctx: '', x: 5, y: 6, tool: '' });
eq((await sam.next('presence')).id, 't1', "students see the teacher's pointer");

// hands and chat
sam.send({ t: 'hand', up: true });
eq((await teacher.next('hands')).hands.map((h) => h.id), ['s1'], 'a raised hand shows to the teacher');
eq((await kim.next('hands')).hands.length, 1, 'and to the class');
await pause(5);
kim.send({ t: 'hand', up: true });
let queue = await teacher.next('hands');
while (queue.hands.length < 2) queue = await teacher.next('hands');
eq(queue.hands.map((h) => h.id), ['s1', 's2'], 'hands queue in the order they went up');
sam.send({ t: 'hand', up: true, who: 's2' });
await pause(60);
kim.send({ t: 'say', text: '  Why is   the survey\n closing 2% off?  ' });
let said = await teacher.next('chat');
while (said.entry.kind !== 'say') said = await teacher.next('chat');
eq([said.entry.name, said.entry.role, said.entry.text], ['Kim', 'student', 'Why is the survey closing 2% off?'], 'a question reaches the teacher, tidied and stamped with who asked');
sam.send({ t: 'say', text: 'one' });
sam.send({ t: 'say', text: 'two' });
eq((await sam.next('denied')).what, 'chat', 'chat is rate limited for students');
teacher.send({ t: 'hand', up: false, who: 's2' });
let after2 = await kim.next('hands');
while (after2.hands.some((h) => h.id === 's2')) after2 = await kim.next('hands');
eq(after2.hands.map((h) => h.id), ['s1'], 'the teacher calls on a student and their hand goes down');
await pause(60);

// the transcript is logged, to the teacher on request and to a file on the server
teacher.send({ t: 'getlog' });
const full = await teacher.next('log');
const texts = full.entries.map((e) => `${e.name}: ${e.text}`);
eq(texts.includes('Sam: raised a hand') && texts.includes('Kim: was called on') && texts.includes('Kim: Why is the survey closing 2% off?'), true, 'the transcript has the hands and the questions');
eq(full.entries.every((e, i) => e.n === i + 1 && typeof e.at === 'number'), true, 'in order, each with a number and a time');
sam.send({ t: 'getlog' });
eq(await sam.quiet('log'), true, 'students cannot ask for the log');
const files = readdirSync(logDir);
eq(files.length, 1, 'one log file for the class');
eq(readFileSync(join(logDir, files[0]), 'utf8').trim().split('\n').length, full.entries.length, 'the file has every entry');

// a student who comes in later is given what was said, and the end of the class stops chat
const mo = await client2();
mo.send({ t: 'join', room: 'Caving 101', id: 's4', name: 'Moe', invite: link, joinOnly: true });
eq((await mo.next('welcome')).class.chat.some((e) => e.text === 'Why is the survey closing 2% off?'), true, 'a latecomer is given the earlier chat');
teacher.send({ t: 'end' });
await sam.next('ended');
await pause(900);
sam.send({ t: 'say', text: 'hello?' });
eq((await sam.next('denied')).message, 'The class has ended.', 'nothing more is said after the class ends');

for (const x of [early, teacher, tooSoon, sam, kim, lena, mo]) x.ws.close();
http2.close();

for (const x of [a, b, c, other, stranger, nobody, samOld, second, samAgain, late, owner2]) x.ws.close();
http.close();
console.log('relay checks ok');
