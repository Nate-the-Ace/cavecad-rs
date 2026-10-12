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
const sam = await client();
sam.send({ t: 'join', room: 'private', id: 'sam1', name: 'Sam', invite: token });
eq((await sam.next('welcome')).owner, false, 'an invite lets one person in');
const second = await client();
second.send({ t: 'join', room: 'private', id: 'two1', invite: token });
eq((await second.next('error')).message.includes('already used'), true, 'a one-person invite is used up');
sam.ws.close();
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

for (const x of [a, b, c, other, stranger, nobody, sam, second, samAgain, late, owner2]) x.ws.close();
http.close();
console.log('relay checks ok');
