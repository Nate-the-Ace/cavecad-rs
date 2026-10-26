// Two devices of one member keep a cave in step through the real server code, in this process.
import { createServer } from 'node:http';
import { generateKeyPairSync } from 'node:crypto';
import { openDb } from '../../server/src/db.ts';
import { handleApi } from '../../server/src/api.ts';
import { lock, makeDeviceKeys, unwrapCaveKey } from '../src/lib/room-crypto.ts';
import { RoomClient, downloadCave, handOverKeys, listMyCaves, type ServerAccess, type CaveEngine, type KeySeen, type Pending, type RoomClientOptions, type RoomStatus } from '../src/lib/room-client.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const until = async (f: () => boolean, ms = 3000) => { const t = Date.now(); while (!f() && Date.now() - t < ms) await new Promise((r) => setTimeout(r, 20)); return f(); };
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** A fake cave: a set of op strings; outbox gives what was added locally since last time. */
type Fake = CaveEngine & { add(op: string): void; ops: Set<string>; applies: number; outboxCalls: number };
function fakeEngine(): Fake {
  let room = '';
  const ops = new Set<string>();
  let out: string[] = [];
  return {
    ops,
    applies: 0,
    outboxCalls: 0,
    add(op) { ops.add(op); out.push(op); },
    roomId: () => room,
    setRoomId: (id) => ((room = id), 'ok'),
    outbox() { this.outboxCalls++; const o = JSON.stringify(out); out = []; return o; },
    apply(s) { this.applies++; const a = JSON.parse(s) as string[]; a.forEach((x) => ops.add(x)); return a.length; },
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
/** What each device keeps between runs (IndexedDB in the app), per cave: newest key seen, unposted entries, synced hash. */
type Kept = { seen: Map<string, KeySeen>; pending: Map<string, Pending[]>; synced: Map<string, string> };
const kept = new Map<string, Kept>();
const keptFor = (dev: Dev): Kept => {
  const k = kept.get(dev.id) ?? { seen: new Map(), pending: new Map(), synced: new Map() };
  kept.set(dev.id, k);
  return k;
};
type Extra = Partial<RoomClientOptions> & { seen?: KeySeen; reasons?: string[]; infos?: string[] };
function client(engine: CaveEngine, dev: Dev, status: RoomStatus[], extra: Extra = {}) {
  const k = keptFor(dev);
  const { seen, reasons, infos, ...opts } = extra;
  return new RoomClient({
    base, fetch, engine, device: dev, flushMs: 50,
    loadKeySeen: async (c) => seen ?? k.seen.get(c) ?? null, saveKeySeen: async (c, s) => void k.seen.set(c, s),
    loadPending: async (c) => {
      const out: Pending[] = [];
      for (const [key, list] of [...k.pending]) if (key.startsWith(c + '|')) (out.push(...list), k.pending.delete(key));
      return out;
    },
    savePending: async (c, who, list) => void (list.length ? k.pending.set(c + '|' + who, [...list]) : k.pending.delete(c + '|' + who)),
    loadSynced: async (c) => k.synced.get(c) ?? null, saveSynced: async (c, h) => void k.synced.set(c, h),
    onStatus: (s, why) => (status.push(s), why && reasons?.push(why)),
    onInfo: (m) => infos?.push(m),
    ...opts,
  });
}
const rows = (c: string) => (db.prepare('SELECT COUNT(*) AS n FROM history WHERE cave_id = ?').get(c) as { n: number }).n;
/** A fetch that answers some requests itself (return a Response) or fails them (throw); the rest go to the server. */
const faulty = (f: (url: string, init: RequestInit | undefined) => Response | Promise<Response> | undefined): typeof fetch =>
  (async (u: RequestInfo | URL, init?: RequestInit) => (await f(String(u), init)) ?? fetch(u, init)) as typeof fetch;
const json = (status: number, body: unknown) => new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });

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
const locked = await lock(key, JSON.stringify({ c: 'someone-else', ops: JSON.stringify(['tampered op']) }), { cave, version: 1, author: laptopDev.id, kind: 'entry' });
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
// Put it back: every later open replays the history from the snapshot, and would stop at it too.
db.prepare('UPDATE history SET author_device = ? WHERE cave_id = ? AND number = ?').run(laptopDev.id, cave, posted.number);

// Two tabs on one device: the same device id and keys (IndexedDB is shared), each with its own cave in memory.
const tabA = fakeEngine();
tabA.setRoomId(cave);
const tabB = fakeEngine();
tabB.setRoomId(cave);
const as: RoomStatus[] = [];
const bs: RoomStatus[] = [];
const ac = client(tabA, laptopDev, as);
const bc = client(tabB, laptopDev, bs);
await ac.open();
await bc.open();
ok(await until(() => as.includes('synced') && bs.includes('synced')), 'two tabs on one device are both synced');
tabA.add('tab A shot');
ok(await until(() => tabB.ops.has('tab A shot'), 2000), 'an edit in tab A reaches tab B on the same device');
tabB.add('tab B sketch');
ok(await until(() => tabA.ops.has('tab B sketch'), 2000), 'an edit in tab B reaches tab A on the same device');
const applied: string[] = [];
const realApply = tabA.apply;
tabA.apply = (s) => (applied.push(s), realApply(s));
tabA.add('tab A only once');
ok(await until(() => tabB.ops.has('tab A only once'), 2000), 'a later tab A edit reaches tab B');
await sleep(200);
ok(!applied.some((s) => s.includes('tab A only once')), 'a tab does not apply its own entry back to itself');
ok(!as.includes('error') && !bs.includes('error'), 'neither tab reports an error');
ac.stop();
bc.stop();

// A wrapped key older than one this device has already used is refused.
const old = fakeEngine();
old.setRoomId(cave);
const os: RoomStatus[] = [];
const oc = client(old, phoneDev, os, { seen: { version: 2, keyId: 'x' } });
await oc.open();
ok(os.at(-1) === 'error', 'a lower key version than seen is refused');
oc.stop();

lc2.stop();
pc2.stop();

// C1: a stopped client never touches the engine again, even in the middle of a long catch-up.
{
  const big = fakeEngine();
  const bs: RoomStatus[] = [];
  const bc0 = client(big, laptopDev, bs, { flushMs: 1e6, snapshotEvery: 1e6 });
  await bc0.open();
  ok(await until(() => bs.includes('synced')), 'C1: the writer is synced');
  for (let i = 0; i < 400; i++) { big.add('op' + i); await bc0.flush(); }
  ok(await until(() => rows(big.roomId()) >= 390, 5000), 'C1: about 400 entries posted');
  bc0.stop();
  const w = fakeEngine();
  w.setRoomId(big.roomId());
  const wc = client(w, tabletDev, [], { flushMs: 20, snapshotEvery: 1e6 });
  void wc.open();
  for (const t = Date.now(); w.applies < 3 && Date.now() - t < 5000; ) await new Promise((r) => setImmediate(r));
  wc.stop();
  ok(w.applies < 100, `C1: stopped mid catch-up (after ${w.applies} applies)`);
  const applies = w.applies;
  const outs = w.outboxCalls;
  await sleep(1500);
  ok(applies >= 3 && w.applies === applies, `C1: no applies after stop (${w.applies - applies} after)`);
  ok(w.outboxCalls === outs, `C1: no outbox drain after stop (${w.outboxCalls - outs} after)`);

  // M3: catching up on 400 entries refreshes the views once, not once per entry.
  const m = fakeEngine();
  m.setRoomId(big.roomId());
  let refreshed = 0;
  const ms: RoomStatus[] = [];
  const mc = client(m, tabletDev, ms, { changed: () => void refreshed++ });
  await mc.open();
  ok(await until(() => ms.includes('synced') && m.ops.has('op399'), 5000), 'M3: the whole history arrives');
  ok(refreshed >= 1 && refreshed <= 2, `M3: views refreshed ${refreshed} times during catch-up`);
  mc.stop();
}

// I2: nothing kept about the last entry is trusted on a new open: a device whose cave was wiped gets it all back.
{
  const wiped = fakeEngine();
  wiped.setRoomId(cave);
  const wc = client(wiped, tabletDev, []);
  await wc.open();
  ok(await until(() => ['trip 1', 'shot A1-A2', 'wall sketch', 'while the tablet was away'].every((x) => wiped.ops.has(x))), 'I2: a wiped copy catches up from the snapshot');
  wc.stop();
}

// I1: edits made in the last flush window are kept at stop and posted by the next open.
{
  const e1 = fakeEngine();
  e1.setRoomId(cave);
  const s1: RoomStatus[] = [];
  const c1 = client(e1, laptopDev, s1, { flushMs: 1e6 });
  await c1.open();
  ok(await until(() => s1.includes('synced')), 'I1: synced');
  e1.add('last-window edit');
  c1.stop();
  await sleep(100);
  const e2 = fakeEngine(); // another tab or a reload: only the kept entries can carry the edit
  e2.setRoomId(cave);
  const c2 = client(e2, laptopDev, []);
  await c2.open();
  ok(await until(() => e2.ops.has('last-window edit')), 'I1: the edit made just before stop reaches the room');
  c2.stop();

  // A copy edited while its room was closed posts itself once on the next open; an unchanged copy posts nothing.
  const e3 = fakeEngine();
  e3.setRoomId(cave);
  const s3: RoomStatus[] = [];
  const c3 = client(e3, laptopDev, s3, { quietMs: 50 });
  await c3.open();
  ok(await until(() => s3.includes('synced')), 'I1: synced again');
  await sleep(300);
  c3.stop();
  await sleep(50);
  const before = rows(cave);
  const s4: RoomStatus[] = [];
  const c4 = client(e3, laptopDev, s4, { quietMs: 50 });
  await c4.open();
  ok(await until(() => s4.includes('synced')), 'I1: reopened');
  await sleep(300);
  c4.stop();
  ok(rows(cave) === before, `I1: an unchanged copy posts nothing on open (${rows(cave) - before} posted)`);
  e3.add('edited while the room was closed');
  e3.outbox(); // lost, as on a reload: only the cave itself still has it
  const c5 = client(e3, laptopDev, []);
  await c5.open();
  const e4 = fakeEngine();
  e4.setRoomId(cave);
  const c6 = client(e4, tabletDev, []);
  await c6.open();
  ok(await until(() => e4.ops.has('edited while the room was closed')), 'I1: an edit made while the room was closed reaches other devices');
  c5.stop();
  c6.stop();
}

// I3: a big outbox goes in pieces under the cap; one op over the cap is an error with a reason, not a retry loop.
{
  const ec = fakeEngine();
  ec.setRoomId(cave);
  const cs: RoomStatus[] = [];
  const why: string[] = [];
  const cc = client(ec, laptopDev, cs, { maxEntryBytes: 4000, reasons: why });
  await cc.open();
  ok(await until(() => cs.includes('synced')), 'I3: synced');
  await sleep(200);
  const before = rows(cave);
  for (let i = 0; i < 50; i++) ec.add(`chunk op ${i} ` + 'x'.repeat(200));
  const ew = fakeEngine();
  ew.setRoomId(cave);
  const cw = client(ew, tabletDev, []);
  await cw.open();
  ok(await until(() => ew.ops.has('chunk op 49 ' + 'x'.repeat(200))), 'I3: a big outbox arrives');
  const fresh = db.prepare('SELECT locked FROM history WHERE cave_id = ? ORDER BY number DESC LIMIT ?').all(cave, rows(cave) - before) as { locked: string }[];
  ok(fresh.length >= 3 && fresh.every((r) => r.locked.length <= 4000), `I3: posted in ${fresh.length} pieces, each under the cap`);
  ec.add('y'.repeat(5000));
  ok(await until(() => cs.includes('error')), 'I3: one op over the cap stops sync');
  ok(why.some((w) => /too large/.test(w)), 'I3: and says why');
  cc.stop();
  cw.stop();
}

// I3: a refused post (400/403/404) is an error and the entry is kept; 401 and network failures are offline, retried.
{
  const er = fakeEngine();
  er.setRoomId(cave);
  const rs: RoomStatus[] = [];
  const refuse = faulty((u, i) => (i?.method === 'POST' && u.endsWith('/entries') ? json(404, { error: 'not-found' }) : undefined));
  const rc = client(er, laptopDev, rs, { fetch: refuse });
  await rc.open();
  ok(await until(() => rs.includes('synced')), 'I3: synced before the refusal');
  er.add('refused op');
  ok(await until(() => rs.includes('error')), 'I3: a 404 on post is an error');
  rc.stop();
  await sleep(50);
  ok([...keptFor(laptopDev).pending].some(([k, l]) => k.startsWith(cave + '|') && l.length > 0), 'I3: the refused entry is kept');

  let left401 = 2;
  let leftNet = 2;
  const flaky = faulty((u, i) => {
    if (i?.method !== 'POST' || !u.endsWith('/entries')) return undefined;
    if (left401 > 0) return (left401--, json(401, { error: 'signed-out' }));
    if (leftNet > 0) { leftNet--; throw new TypeError('fetch failed'); }
    return undefined;
  });
  const ef = fakeEngine();
  ef.setRoomId(cave);
  const fs: RoomStatus[] = [];
  const fc = client(ef, laptopDev, fs, { fetch: flaky });
  await fc.open(); // posts the kept refused entry first, through the flaky fetch
  ef.add('flaky op');
  const ew = fakeEngine();
  ew.setRoomId(cave);
  const cw = client(ew, tabletDev, []);
  await cw.open();
  ok(await until(() => ew.ops.has('flaky op') && ew.ops.has('refused op'), 5000), 'I3: after 401s and network failures the edits arrive');
  ok(fs.includes('offline') && !fs.includes('error'), 'I3: 401 and network failures show offline, not error');
  fc.stop();
  cw.stop();
}

// I5: a cave whose room is not this member's (another account's file) gets a new room, said once.
{
  const ex = fakeEngine();
  ex.setRoomId('cave_' + 'f'.repeat(24));
  ex.add('their cave, my copy');
  const xs: RoomStatus[] = [];
  const infos: string[] = [];
  const xc = client(ex, laptopDev, xs, { infos });
  await xc.open();
  ok(await until(() => xs.includes('synced')), 'I5: synced in a new room');
  ok(ex.roomId() !== 'cave_' + 'f'.repeat(24) && ex.roomId().startsWith('cave_'), 'I5: the cave has a new room id');
  ok(infos.length === 1 && /another account or device/.test(infos[0]), 'I5: said once');
  ok(!xs.includes('waiting-key'), 'I5: does not wait for a key that will never come');
  const ey = fakeEngine();
  ey.setRoomId(ex.roomId());
  const yc = client(ey, tabletDev, []);
  await yc.open();
  ok(await until(() => ey.ops.has('their cave, my copy')), 'I5: the new room has the copy');
  xc.stop();
  yc.stop();
}

// I6: a stream with nothing on it for too long is dropped and reopened.
{
  let streams = 0;
  const counting = faulty((u) => void (u.includes('/stream') && streams++));
  const ed = fakeEngine();
  ed.setRoomId(cave);
  const dc = client(ed, laptopDev, [], { fetch: counting, deadAfterMs: 300, reconnectMs: 50 });
  await dc.open();
  await sleep(1300);
  ok(streams >= 3, `I6: a silent stream is reopened (${streams} streams)`);
  const n = rows(cave);
  ed.add('after a reconnect');
  ok(await until(() => rows(cave) > n), 'I6: still syncing');
  dc.stop();
}

// M1: the room id is set before the room is made, so a retry after a lost answer reuses it.
{
  let lose = 1;
  const lossy = faulty(async (u, i) => {
    if (i?.method === 'POST' && u.endsWith('/api/caves') && lose > 0) { lose--; await fetch(u, i); throw new TypeError('answer lost'); }
    return undefined;
  });
  const en = fakeEngine();
  en.add('made while the answer was lost');
  const ns: RoomStatus[] = [];
  const nc = client(en, laptopDev, ns, { fetch: lossy, openRetryMs: 100 });
  await nc.open();
  const first = en.roomId();
  ok(first.startsWith('cave_'), 'M1: the room id is set before posting');
  ok(await until(() => ns.includes('synced'), 5000), 'M1: synced after retrying');
  ok(en.roomId() === first, 'M1: the retry kept the same room id');
  const eo = fakeEngine();
  eo.setRoomId(first);
  const oc2 = client(eo, tabletDev, []);
  await oc2.open();
  ok(await until(() => eo.ops.has('made while the answer was lost')), 'M1: another device gets the cave');
  nc.stop();
  oc2.stop();
}
// M1: a first snapshot that failed to upload is uploaded on the next open.
{
  let failSnap = 1;
  const snapFails = faulty((u, i) => (i?.method === 'PUT' && u.endsWith('/snapshot') && failSnap > 0 ? (failSnap--, json(500, { error: 'error' })) : undefined));
  const en = fakeEngine();
  en.add('made while the snapshot failed');
  const ns: RoomStatus[] = [];
  const nc = client(en, laptopDev, ns, { fetch: snapFails, openRetryMs: 100 });
  await nc.open();
  ok(await until(() => ns.includes('synced'), 5000), 'M1: synced after the snapshot failed');
  ok(await until(() => !!db.prepare('SELECT 1 FROM snapshots WHERE cave_id = ?').get(en.roomId())), 'M1: the first snapshot is uploaded on the retry');
  const eo = fakeEngine();
  eo.setRoomId(en.roomId());
  const oc2 = client(eo, tabletDev, []);
  await oc2.open();
  ok(await until(() => eo.ops.has('made while the snapshot failed')), 'M1: another device gets that cave');
  nc.stop();
  oc2.stop();
}

// M4: a device whose public key does not parse is skipped when wrapping, not fatal.
{
  const member = (db.prepare('SELECT member_id FROM devices WHERE id = ?').get(laptopDev.id) as { member_id: string }).member_id;
  db.prepare('INSERT INTO devices (id, member_id, public_key, created_at, last_seen_at) VALUES (?, ?, ?, 0, 0)').run('bad-dev', member, 'not-a-key');
  const eb = fakeEngine();
  const bs: RoomStatus[] = [];
  const bc = client(eb, laptopDev, bs);
  await bc.open();
  ok(await until(() => bs.includes('synced')), 'M4: a room is made despite a device with a bad key');
  bc.stop();
  db.prepare("DELETE FROM devices WHERE id = 'bad-dev'").run();
}

// M5: retrying while waiting for a key does not flicker back to "connecting".
{
  const late = await device();
  const ek = fakeEngine();
  ek.setRoomId(cave);
  const ks: RoomStatus[] = [];
  const kc = client(ek, late, ks, { keyRetryMs: 100 });
  await kc.open();
  await sleep(450);
  kc.stop();
  ok(ks.filter((x) => x === 'connecting').length === 1 && ks.at(-1) === 'waiting-key', `M5: statuses ${ks.join(',')}`);
}

// M6: only a client that wrote entries since the last snapshot uploads the next one.
{
  const puts = { a: 0, b: 0 };
  const count = (who: 'a' | 'b') => faulty((u, i) => void (i?.method === 'PUT' && u.endsWith('/snapshot') && puts[who]++));
  const ea = fakeEngine();
  const as2: RoomStatus[] = [];
  const ca = client(ea, laptopDev, as2, { fetch: count('a'), snapshotEvery: 5, flushMs: 1e6 });
  await ca.open();
  ok(await until(() => as2.includes('synced')), 'M6: writer synced');
  const eb = fakeEngine();
  eb.setRoomId(ea.roomId());
  const bs2: RoomStatus[] = [];
  const cb = client(eb, tabletDev, bs2, { fetch: count('b'), snapshotEvery: 5 });
  await cb.open();
  ok(await until(() => bs2.includes('synced')), 'M6: reader synced');
  puts.a = puts.b = 0;
  for (let i = 0; i < 12; i++) { ea.add('m6 ' + i); await ca.flush(); }
  ok(await until(() => eb.ops.has('m6 11')), 'M6: the reader has every entry');
  await sleep(200);
  ok(puts.a >= 1 && puts.b === 0, `M6: snapshots uploaded by writer ${puts.a}, reader ${puts.b}`);
  ca.stop();
  cb.stop();
}

let relabelled = false;

// The Cave Shelf on a device that does not keep a cave yet: list it by its locked label, wait for a key, open it.
{
  const laptopB = await device();
  const eng = fakeEngine();
  eng.add('Truitt trip 1');
  let label = JSON.stringify({ name: 'Truitt Cave' });
  const st: RoomStatus[] = [];
  const c1 = client(eng, laptopB, st, { label: () => label, quietMs: 50 });
  await c1.open();
  ok(await until(() => st.includes('synced')), 'S1: laptop synced');
  const truitt = eng.roomId();
  eng.add('Truitt trip 2');
  ok(await until(() => rows(truitt) >= 1), 'S1: an edit was posted');

  const workDev = await device();
  const access = (dev: Dev): ServerAccess => {
    const k = keptFor(dev);
    return { base, fetch, device: dev, loadKeySeen: async (c) => k.seen.get(c) ?? null, saveKeySeen: async (c, s) => void k.seen.set(c, s) };
  };
  let list = await listMyCaves(access(workDev));
  let t = list.find((x) => x.id === truitt);
  ok(!!t && !t.hasKey && t.label === null, 'S2: the work PC lists the cave, with no key and so no name yet');
  ok((await downloadCave(access(workDev), truitt)).status === 'waiting', 'S2: opening it waits for a key');
  ok((await downloadCave(access(workDev), 'cave_nosuchcave000')).status === 'unknown', 'S2: a cave that is not the member\'s is unknown');

  // The laptop hands its keys on without opening the cave again.
  ok((await handOverKeys(access(laptopB))) >= 1, 'S3: the laptop hands the key on');
  ok((await handOverKeys(access(laptopB))) === 0, 'S3: nothing is left to hand on');
  list = await listMyCaves(access(workDev));
  t = list.find((x) => x.id === truitt);
  ok(!!t && t.hasKey && t.label === label, `S3: the work PC now reads the name (${t?.label})`);

  const got = await downloadCave(access(workDev), truitt);
  ok(got.status === 'ok', 'S4: the work PC downloads the cave');
  if (got.status === 'ok') {
    const work = fakeEngine();
    work.load(got.snapshot ?? '[]');
    for (const e of got.entries) work.apply(e);
    ok(work.ops.has('Truitt trip 1') && work.ops.has('Truitt trip 2'), 'S4: snapshot and history make the whole cave');
  }

  // A renamed cave: its new label is kept once the cave has been still.
  label = JSON.stringify({ name: 'Truitt Cave (main)' });
  eng.add('Truitt trip 3');
  ok(await until(() => {
    void listMyCaves(access(workDev)).then((l) => (relabelled = l.find((x) => x.id === truitt)?.label === label));
    return relabelled;
  }, 4000), 'S5: a new name reaches the server, locked');
  const labels = (db.prepare('SELECT locked FROM cave_labels').all() as { locked: string }[]).map((r) => r.locked);
  ok(labels.length >= 1 && labels.every((x) => !/Truitt/.test(x) && !/Truitt/.test(Buffer.from(x, 'base64url').toString())), 'S5: the server cannot read the label');
  c1.stop();
}

server.close();
console.log(bad ? `roomclient: ${bad} failed` : 'roomclient: all ok');
process.exit(bad ? 1 : 0);
