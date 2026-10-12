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

for (const x of [a, b, c, other]) x.ws.close();
http.close();
console.log('relay checks ok');
