// Two devices of one member keep a cave in step through the real server code, in this process.
import { createServer } from 'node:http';
import { generateKeyPairSync } from 'node:crypto';
import { openDb } from '../../server/src/db.ts';
import { handleApi } from '../../server/src/api.ts';
import { lock, makeDeviceKeys, unwrapCaveKey } from '../src/lib/room-crypto.ts';
import { RoomClient, type CaveEngine, type KeySeen, type RoomStatus } from '../src/lib/room-client.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const until = async (f: () => boolean, ms = 3000) => { const t = Date.now(); while (!f() && Date.now() - t < ms) await new Promise((r) => setTimeout(r, 20)); return f(); };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** A fake cave: a set of op strings; outbox gives what was added locally since last time. */
function fakeEngine(): CaveEngine & { add(op: string): void; ops: Set<string> } {
  let room = '';
  const ops = new Set<string>();
  let out: string[] = [];
  return {
    ops,
    add(op) { ops.add(op); out.push(op); },
    roomId: () => room,
    setRoomId: (id) => ((room = id), 'ok'),
    outbox: () => { const o = JSON.stringify(out); out = []; return o; },
    apply: (s) => { const a = JSON.parse(s) as string[]; a.forEach((x) => ops.add(x)); return a.length; },
    snapshot: () => JSON.stringify([...ops]),
    load: (s) => { ops.clear(); (JSON.parse(s) as string[]).forEach((x) => ops.add(x)); return 'ok'; },
  };
}

const db = openDb(':memory:');
const server = createServer((req, res) => void handleApi({
  db, adminEmail: 'nathan@example.com', passKey: generateKeyPairSync('ed25519').privateKey, publicUrl: 'http://127.0.0.1', mailer: { url: '', key: '' },
  whoIs: async () => ({ email: 'nathan@example.com', name: 'Nathan', provider: null, subject: '' }),
}, req, res));
await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;

type Dev = { id: string; privateKey: CryptoKey; publicKey: string };
async function device(): Promise<Dev> {
  const keys = await makeDeviceKeys();
  const res = await fetch(base + '/api/devices', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ publicKey: keys.publicKey }) });
  return { id: (await res.json()).device.id as string, privateKey: keys.privateKey, publicKey: keys.publicKey };
}
/** What each device keeps between runs (IndexedDB in the app): last applied number and newest key seen, per cave. */
const kept = new Map<string, { last: Map<string, number>; seen: Map<string, KeySeen> }>();
function client(engine: CaveEngine, dev: Dev, status: RoomStatus[], seenOverride?: KeySeen) {
  const k = kept.get(dev.id) ?? { last: new Map(), seen: new Map() };
  kept.set(dev.id, k);
  return new RoomClient({
    base, fetch, engine, device: dev, flushMs: 50,
    loadLast: async (c) => k.last.get(c) ?? 0, saveLast: async (c, n) => void k.last.set(c, n),
    loadKeySeen: async (c) => seenOverride ?? k.seen.get(c) ?? null, saveKeySeen: async (c, s) => void k.seen.set(c, s),
    onStatus: (s) => status.push(s),
  });
}

const laptopDev = await device();
const tabletDev = await device();
const laptop = fakeEngine();
laptop.add('trip 1');
const ls: RoomStatus[] = [];
const lc = client(laptop, laptopDev, ls);
await lc.open();
ok(laptop.roomId().startsWith('cave_'), 'the laptop made the room and set its id in the cave');
ok(await until(() => ls.includes('synced')), 'laptop is synced');
const cave = laptop.roomId();

const tablet = fakeEngine();
tablet.setRoomId(cave); // the same cave file opened on the tablet
const ts: RoomStatus[] = [];
const tc = client(tablet, tabletDev, ts);
await tc.open();
ok(await until(() => tablet.ops.has('trip 1')), 'the tablet gets the cave from the snapshot');
ok(await until(() => ts.includes('synced')), 'tablet is synced');

laptop.add('shot A1-A2');
ok(await until(() => tablet.ops.has('shot A1-A2'), 2000), 'a laptop edit reaches the tablet within 2 s');
tablet.add('wall sketch');
ok(await until(() => laptop.ops.has('wall sketch'), 2000), 'a tablet edit reaches the laptop within 2 s');

tc.stop();
laptop.add('while the tablet was away');
await sleep(200);
const tc2 = client(tablet, tabletDev, []);
await tc2.open();
ok(await until(() => tablet.ops.has('while the tablet was away')), 'the tablet catches up when it comes back');

// A device registered after the room was made has no key until one of the member's devices with it opens the cave.
const phoneDev = await device();
const phone = fakeEngine();
phone.setRoomId(cave);
const ps: RoomStatus[] = [];
const pc = client(phone, phoneDev, ps);
await pc.open();
ok(ps.at(-1) === 'waiting-key', 'a device with no wrapped key waits for one');
pc.stop();
lc.stop();
const ls2: RoomStatus[] = [];
const lc2 = client(laptop, laptopDev, ls2);
await lc2.open(); // hands the key to the phone
ok(await until(() => ls2.includes('synced')), 'the reopened laptop is synced');
const pc2 = client(phone, phoneDev, ps);
await pc2.open();
ok(await until(() => ['trip 1', 'shot A1-A2', 'wall sketch', 'while the tablet was away'].every((x) => phone.ops.has(x))), 'the phone gets the key and the whole cave');
ok(ps.at(-1) === 'synced', 'the phone is synced');

const stored = [
  ...(db.prepare('SELECT locked FROM history').all() as { locked: string }[]).map((r) => r.locked),
  ...(db.prepare('SELECT locked FROM snapshots').all() as { locked: string }[]).map((r) => r.locked),
  ...(db.prepare('SELECT wrapped FROM wrapped_keys').all() as { wrapped: string }[]).map((r) => r.wrapped),
];
ok(stored.length > 3 && stored.every((t) => !/shot|trip|wall|tablet|upTo/.test(t)), 'the server stores nothing readable');

// An entry the server relabels as the receiving device's own must fail to unlock, not be skipped as "own".
tc2.stop();
await sleep(100);
const myCaves = await (await fetch(base + '/api/caves', { headers: { 'X-Cave-Device': laptopDev.id } })).json();
const w = myCaves.caves.find((c: { id: string }) => c.id === cave);
const { key } = await unwrapCaveKey(w.wrapped, laptopDev.privateKey, { cave, version: w.keyVersion, myPublicKey: laptopDev.publicKey });
const locked = await lock(key, JSON.stringify(['tampered op']), { cave, version: 1, author: laptopDev.id, kind: 'entry' });
const posted = await (await fetch(base + `/api/caves/${cave}/entries`, { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Cave-Device': laptopDev.id }, body: JSON.stringify({ version: 1, locked }) })).json();
ok(await until(() => phone.ops.has('tampered op')), 'the genuine entry reaches the phone');
db.prepare('UPDATE history SET author_device = ? WHERE cave_id = ? AND number = ?').run(tabletDev.id, cave, posted.number);
const ts3: RoomStatus[] = [];
const tc3 = client(tablet, tabletDev, ts3);
await tc3.open();
ok(await until(() => ts3.includes('error')), 'a relabelled entry is reported as an error');
ok(!tablet.ops.has('tampered op'), 'a relabelled entry is not applied');
ok(!ts3.includes('synced'), 'the tablet does not claim to be synced past it');
tc3.stop();

// A wrapped key older than one this device has already used is refused.
const old = fakeEngine();
old.setRoomId(cave);
const os: RoomStatus[] = [];
const oc = client(old, phoneDev, os, { version: 2, keyId: 'x' });
await oc.open();
ok(os.at(-1) === 'error', 'a lower key version than seen is refused');
oc.stop();

lc2.stop();
pc2.stop();
server.close();
console.log(bad ? `roomclient: ${bad} failed` : 'roomclient: all ok');
process.exit(bad ? 1 : 0);
