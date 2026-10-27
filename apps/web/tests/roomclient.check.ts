// Two devices of one member keep a cave in step through the real server code, in this process. New caves are open
// (plain text on the server, no keys); caves locked during rooms step 1 convert when a device with their key opens them.
import { createServer } from 'node:http';
import { createHash, generateKeyPairSync } from 'node:crypto';
import { openDb } from '../../server/src/db.ts';
import { handleApi } from '../../server/src/api.ts';
import { lock, makeCaveKey, makeDeviceKeys, wrapCaveKey } from '../src/lib/room-crypto.ts';
import { createInvite } from '../../server/src/members.ts';
import { addRoom, removeGuest, setGuest } from '../../server/src/rooms.ts';
import { RoomClient, QUARANTINE_INFO, REMOVED, type Role, downloadCave, handOverKeys, listMyCaves, type ServerAccess, type CaveEngine, type KeySeen, type Pending, type RoomClientOptions, type RoomStatus } from '../src/lib/room-client.ts';

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
  // Nathan unless a test names another member (a guest) in X-Test-User.
  whoIs: async (r) => {
    const e = String(r.headers['x-test-user'] ?? 'nathan@example.com');
    return { email: e, name: e.split('@')[0], provider: null, subject: '' };
  },
}, req, res));
await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;

type Dev = { id: string; privateKey: CryptoKey; publicKey: string };
async function device(user = ''): Promise<Dev> {
  const keys = await makeDeviceKeys();
  const res = await fetch(base + '/api/devices', { method: 'POST', headers: { 'Content-Type': 'application/json', ...(user ? { 'x-test-user': user } : {}) }, body: JSON.stringify({ publicKey: keys.publicKey }) });
  return { id: (await res.json()).device.id as string, privateKey: keys.privateKey, publicKey: keys.publicKey };
}
/** What each device keeps between runs (IndexedDB in the app), per cave: newest key seen, unposted entries, synced hash. */
type Kept = { seen: Map<string, KeySeen>; pending: Map<string, Pending[]>; synced: Map<string, string>; quarantine: Map<string, Pending[]> };
const kept = new Map<string, Kept>();
const keptFor = (dev: Dev): Kept => {
  const k = kept.get(dev.id) ?? { seen: new Map(), pending: new Map(), synced: new Map(), quarantine: new Map() };
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
    saveQuarantine: async (c, list) => void k.quarantine.set(c, [...(k.quarantine.get(c) ?? []), ...list]),
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

// A device registered after the room was made needs no key: it opens an open cave at once.
const phoneDev = await device();
const phone = fakeEngine();
phone.setRoomId(cave);
const ps: RoomStatus[] = [];
const pc2 = client(phone, phoneDev, ps);
await pc2.open();
ok(await until(() => ['trip 1', 'shot A1-A2', 'wall sketch', 'while the tablet was away'].every((x) => phone.ops.has(x))), 'O1: a device that never had a key gets the whole cave');
ok(!ps.includes('waiting-key') && ps.at(-1) === 'synced', `O1: the new device is synced without waiting (${ps.join(',')})`);
lc.stop();
const ls2: RoomStatus[] = [];
const lc2 = client(laptop, laptopDev, ls2);
await lc2.open();
ok(await until(() => ls2.includes('synced')), 'the reopened laptop is synced');

// New caves are open: stored as plain JSON text, with no keys anywhere.
const caveRow = db.prepare('SELECT storage, key_version FROM caves WHERE id = ?').get(cave) as { storage: string; key_version: number };
ok(caveRow.storage === 'open' && caveRow.key_version === 0, `O2: a new cave is open (${caveRow.storage})`);
ok((db.prepare('SELECT COUNT(*) AS n FROM wrapped_keys').get() as { n: number }).n === 0, 'O2: no cave key was made or wrapped');
const histTexts = (db.prepare('SELECT locked, key_version FROM history WHERE cave_id = ?').all(cave) as { locked: string; key_version: number }[]);
ok(histTexts.length > 0 && histTexts.every((r) => {
  const b = JSON.parse(r.locked);
  return r.key_version === 0 && typeof b.c === 'string' && typeof b.ops === 'string' && Array.isArray(JSON.parse(b.ops));
}), 'O2: each entry is the plain JSON {c, ops}');
ok(histTexts.some((r) => r.locked.includes('shot A1-A2')), 'O2: the server can read the edits');
const snapRow = db.prepare('SELECT up_to, locked FROM snapshots WHERE cave_id = ?').get(cave) as { up_to: number; locked: string };
const snapBody = JSON.parse(snapRow.locked);
ok(snapBody.upTo === snapRow.up_to && typeof snapBody.ops === 'string' && snapBody.ops.includes('trip 1'), 'O2: the snapshot is the plain JSON {upTo, ops} with a matching number');

// An entry from another client is applied; an entry that is not {c, ops} stops sync with an error and is not applied.
tc2.stop();
await sleep(100);
const postPlain = async (c: string, text: string) =>
  (await (await fetch(base + `/api/caves/${c}/entries`, { method: 'POST', headers: { 'Content-Type': 'application/json', 'X-Cave-Device': laptopDev.id }, body: JSON.stringify({ locked: text }) })).json()) as { number: number };
await postPlain(cave, JSON.stringify({ c: 'someone-else', ops: JSON.stringify(['other client op']) }));
ok(await until(() => phone.ops.has('other client op')), 'O3: an entry from another client reaches the phone');
const posted = await postPlain(cave, JSON.stringify({ c: 'someone-else', ops: 42 }));
const ts3: RoomStatus[] = [];
const tc3 = client(tablet, tabletDev, ts3);
await tc3.open();
ok(await until(() => ts3.includes('error')), 'O3: a malformed entry is reported as an error');
ok(tablet.ops.has('other client op'), 'O3: entries before it were applied');
ok(!ts3.includes('synced'), 'O3: the tablet does not claim to be synced past it');
tc3.stop();
// Put it right: every later open replays the history from the snapshot, and would stop at it too.
db.prepare('UPDATE history SET locked = ? WHERE cave_id = ? AND number = ?').run(JSON.stringify({ c: 'someone-else', ops: '[]' }), cave, posted.number);

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

// M4: a device whose public key does not parse does not stop a room being made (no wrapping for open caves).
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

// The Cave Shelf on a device that does not keep a cave yet: list it by its plain label and open it, no key needed.
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
  ok(await until(() => !!db.prepare('SELECT 1 FROM cave_labels WHERE cave_id = ?').get(truitt)), 'S1: the label was kept');
  const list = await listMyCaves(access(workDev));
  const t = list.find((x) => x.id === truitt);
  ok(!!t && t.hasKey && t.open && t.label === label, `S2: the work PC, which never had a key, lists the cave by name (${t?.label})`);
  ok((await downloadCave(access(workDev), 'cave_nosuchcave000')).status === 'unknown', 'S2: a cave that is not the member\'s is unknown');
  ok((await handOverKeys(access(laptopB))) === 0, 'S3: an open cave has no keys to hand on');

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
  }, 4000), 'S5: a new name reaches the server');
  const kept = db.prepare('SELECT key_version, locked FROM cave_labels WHERE cave_id = ?').get(truitt) as { key_version: number; locked: string };
  ok(kept.key_version === 0 && kept.locked === label, 'S5: the label is kept as plain JSON');
  c1.stop();
}

// Caves locked during rooms step 1: made here the old way (a cave key wrapped for one device, locked entries).
const hdr = (dev: Dev) => ({ 'Content-Type': 'application/json', 'X-Cave-Device': dev.id });
async function lockedCave(owners: Dev[], ops: string[], label?: string): Promise<{ id: string; key: CryptoKey }> {
  const made = await makeCaveKey();
  const id = 'cave_' + Math.random().toString(16).slice(2, 14).padEnd(12, '0') + 'aa';
  const wrapped = [];
  for (const d of owners) wrapped.push({ device: d.id, wrapped: await wrapCaveKey(made.raw, d.publicKey, { cave: id, version: 1 }) });
  const r = await fetch(base + '/api/caves', { method: 'POST', headers: hdr(owners[0]), body: JSON.stringify({ id, wrapped }) });
  if (!r.ok) throw new Error('locked cave not made: ' + r.status);
  for (const op of ops) {
    const locked = await lock(made.key, JSON.stringify({ c: 'old-client', ops: JSON.stringify([op]) }), { cave: id, version: 1, author: owners[0].id, kind: 'entry' });
    await fetch(base + `/api/caves/${id}/entries`, { method: 'POST', headers: hdr(owners[0]), body: JSON.stringify({ version: 1, locked }) });
  }
  if (label) {
    const locked = await lock(made.key, label, { cave: id, version: 1, author: 'label', kind: 'label' });
    await fetch(base + `/api/caves/${id}/label`, { method: 'PUT', headers: hdr(owners[0]), body: JSON.stringify({ version: 1, locked }) });
  }
  return { id, key: made.key };
}
const storageOf = (id: string) => (db.prepare('SELECT storage FROM caves WHERE id = ?').get(id) as { storage: string }).storage;

{
  const keyed = await device();
  const old = await lockedCave([keyed], ['locked op 1', 'locked op 2', 'locked op 3'], JSON.stringify({ name: 'Old Locked Cave' }));
  ok(storageOf(old.id) === 'locked' && rows(old.id) === 3, 'L0: a locked cave with three entries');

  // M5: a device with no key waits (until the cave converts), without flickering back to "connecting".
  const late = await device();
  const ek = fakeEngine();
  ek.setRoomId(old.id);
  const ks: RoomStatus[] = [];
  const kc = client(ek, late, ks, { keyRetryMs: 100 });
  await kc.open();
  await sleep(450);
  ok(ks.filter((x) => x === 'connecting').length === 1 && ks.at(-1) === 'waiting-key', `M5: statuses ${ks.join(',')}`);

  // A wrapped key older than one this device has already used is refused (the locked path keeps its checks).
  const ev = fakeEngine();
  ev.setRoomId(old.id);
  const vs: RoomStatus[] = [];
  const vc = client(ev, keyed, vs, { seen: { version: 2, keyId: 'x' } });
  await vc.open();
  ok(vs.at(-1) === 'error', 'L1: a lower key version than seen is refused');
  ok(storageOf(old.id) === 'locked', 'L1: and the cave is not converted');
  vc.stop();
  keptFor(keyed).seen.delete(old.id);

  // An edit made offline before the move, kept on the keyed device in the locked format by another tab.
  const pendingLocked = await lock(old.key, JSON.stringify({ c: 'old-tab', ops: JSON.stringify(['offline locked op']) }), { cave: old.id, version: 1, author: keyed.id, kind: 'entry' });
  keptFor(keyed).pending.set(old.id + '|old-tab', [{ version: 1, locked: pendingLocked }]);
  // Locked-format posts fail (as if offline) so the kept entry can only reach the room after conversion.
  const noLockedPosts = faulty((u, i) => (i?.method === 'POST' && u.endsWith('/entries') && JSON.parse(String(i.body)).version ? json(503, { error: 'error' }) : undefined));
  const el = fakeEngine();
  el.setRoomId(old.id);
  const lls: RoomStatus[] = [];
  const why: string[] = [];
  const lcl = client(el, keyed, lls, { fetch: noLockedPosts, reasons: why, label: () => JSON.stringify({ name: 'Old Locked Cave' }), quietMs: 50 });
  await lcl.open();
  ok(await until(() => storageOf(old.id) === 'open', 5000), 'L2: the keyed device converts the cave when it opens it');
  ok(['locked op 1', 'locked op 2', 'locked op 3'].every((x) => el.ops.has(x)), 'L2: it caught up on the locked history first');
  ok((db.prepare('SELECT COUNT(*) AS n FROM wrapped_keys WHERE cave_id = ?').get(old.id) as { n: number }).n === 0, 'L2: the wrapped keys are gone');
  const osnap = db.prepare('SELECT up_to, key_version, locked FROM snapshots WHERE cave_id = ?').get(old.id) as { up_to: number; key_version: number; locked: string };
  const ob = JSON.parse(osnap.locked);
  ok(osnap.up_to === 3 && osnap.key_version === 0 && ob.upTo === 3 && ob.ops.includes('locked op 2'), `L2: an open snapshot at the last number (${osnap.up_to})`);
  ok(await until(() => el.ops.has('offline locked op') || rows(old.id) > 0, 5000), 'L3: the kept locked entry is posted after conversion');
  const firstOpen = db.prepare('SELECT number, key_version, locked FROM history WHERE cave_id = ? ORDER BY number LIMIT 1').get(old.id) as { number: number; key_version: number; locked: string } | undefined;
  ok(!!firstOpen && firstOpen.number === 4 && firstOpen.key_version === 0, `L3: entry numbers continue after the snapshot (${firstOpen?.number})`);
  ok(!!firstOpen && JSON.parse(firstOpen.locked).ops.includes('offline locked op'), 'L3: re-posted as an open entry, unlocked with the key held in memory');
  ok(await until(() => lls.at(-1) === 'synced', 5000) && !lls.includes('error'), `L2: the converting device is synced (${lls.join(',')} ${why.join(';')})`);
  ok(await until(() => (db.prepare('SELECT key_version FROM cave_labels WHERE cave_id = ?').get(old.id) as { key_version: number } | undefined)?.key_version === 0, 3000), 'L2: the label is kept again, plain');

  // The device that was waiting for a key now syncs it with none.
  ok(await until(() => ['locked op 1', 'locked op 3', 'offline locked op'].every((x) => ek.ops.has(x)), 5000), 'L4: the device that was waiting gets the cave without a key');
  el.add('after conversion');
  ok(await until(() => ek.ops.has('after conversion'), 3000), 'L4: and later edits');
  ok(!ks.includes('error'), 'L4: no error on the waiting device');
  const fresh = await device();
  const ef = fakeEngine();
  ef.setRoomId(old.id);
  const fs: RoomStatus[] = [];
  const fc = client(ef, fresh, fs);
  await fc.open();
  ok(await until(() => ['locked op 2', 'offline locked op', 'after conversion'].every((x) => ef.ops.has(x))), 'L4: a second device syncs the converted cave with no key');
  ef.add('from the second device');
  ok(await until(() => el.ops.has('from the second device'), 3000), 'L4: edits flow back to the converting device');
  const freshAccess: ServerAccess = { base, fetch, device: fresh, loadKeySeen: async () => null, saveKeySeen: async () => {} };
  const dl = await downloadCave(freshAccess, old.id);
  ok(dl.status === 'ok' && !!dl.snapshot?.includes('locked op 1') && dl.entries.some((e) => e.includes('after conversion')), 'L4: the converted cave downloads with no key');
  kc.stop();
  fc.stop();
  lcl.stop();

  // L5: an open cave with locked entries kept on a device that has no key (offline across the conversion). They are
  // moved aside into quarantine, never deleted; the device's whole copy is sent as an open entry instead (even though
  // its synced hash says nothing changed), the user is told, and sync carries on open.
  const stray = await device();
  keptFor(stray).pending.set(old.id + '|gone-tab', [{ version: 1, locked: pendingLocked }]);
  const es = fakeEngine();
  es.setRoomId(old.id);
  es.add('stray offline edit');
  es.outbox(); // already in the copy, as after a reload: only the whole-copy resend can carry it
  keptFor(stray).synced.set(old.id, createHash('sha256').update(es.snapshot()).digest('hex'));
  const ss: RoomStatus[] = [];
  const sw: string[] = [];
  const si: string[] = [];
  const sc = client(es, stray, ss, { reasons: sw, infos: si });
  await sc.open();
  ok(await until(() => ss.at(-1) === 'synced', 5000) && !ss.includes('error'), `L5: a device with unreadable locked edits still syncs (${ss.join(',')} ${sw.join(';')})`);
  ok(si.includes(QUARANTINE_INFO), `L5: the user is told the edits were re-sent (${si.join(';')})`);
  ok((keptFor(stray).quarantine.get(old.id) ?? []).some((p) => p.locked === pendingLocked), 'L5: the locked entries are kept in quarantine');
  ok(![...keptFor(stray).pending].some(([, l]) => l.some((p) => p.version !== 0)), 'L5: and are no longer pending');
  const ep = fakeEngine();
  ep.setRoomId(old.id);
  const pcs: RoomStatus[] = [];
  const pc5 = client(ep, await device(), pcs);
  await pc5.open();
  ok(await until(() => ep.ops.has('stray offline edit'), 3000), 'L5: the edits arrive on another device');
  sc.stop();
  pc5.stop();
}

// L7: in open mode a stream line under a key version is a reason to read again, not a failure.
{
  const eo = fakeEngine();
  eo.add('m1 open cave');
  const os: RoomStatus[] = [];
  const ow: string[] = [];
  let injected = 0;
  let streams = 0;
  const lockedLine = faulty((u, i) => {
    if (!u.includes('/stream?')) return undefined;
    streams++;
    if (injected) return undefined;
    injected++;
    const body = JSON.stringify({ t: 'entry', number: 999, keyVersion: 1, locked: 'AAAA', authorDevice: laptopDev.id }) + '\n';
    return new Response(body, { status: 200, headers: { 'Content-Type': 'application/x-ndjson' } });
  });
  const oc = client(eo, laptopDev, os, { fetch: lockedLine, reasons: ow, reconnectMs: 50 });
  await oc.open();
  ok(await until(() => injected === 1 && streams >= 2 && os.at(-1) === 'synced', 3000), `L7: a locked line in an open cave reconnects (${os.join(',')} ${ow.join(';')})`);
  ok(!os.includes('error'), 'L7: and is not a failure');
  const other = fakeEngine();
  other.setRoomId(eo.roomId());
  const oc3 = client(other, tabletDev, []);
  await oc3.open();
  eo.add('m1 after reconnect');
  ok(await until(() => other.ops.has('m1 after reconnect'), 3000), 'L7: sync carries on after the reconnect');
  oc.stop();
  oc3.stop();
}

// L6: two keyed devices; another converts first, so this one's conversion is refused: it reloads and goes on open.
{
  const a = await device();
  const b = await device();
  const raced = await lockedCave([a, b], ['race op 1', 'race op 2']);
  let raceDone = false;
  const otherFirst = faulty(async (u, i) => {
    if (!raceDone && i?.method === 'POST' && u.endsWith(`/api/caves/${raced.id}/open`)) {
      raceDone = true;
      const body = JSON.parse(String(i.body)) as { upTo: number };
      const snapshot = JSON.stringify({ upTo: body.upTo, ops: JSON.stringify(['race op 1', 'race op 2', 'from the other device']) });
      await fetch(u, { method: 'POST', headers: hdr(a), body: JSON.stringify({ upTo: body.upTo, snapshot }) });
    }
    return undefined;
  });
  const eb = fakeEngine();
  eb.setRoomId(raced.id);
  const bs: RoomStatus[] = [];
  const bw: string[] = [];
  const bc = client(eb, b, bs, { fetch: otherFirst, reasons: bw });
  await bc.open();
  ok(await until(() => raceDone && storageOf(raced.id) === 'open'), 'L6: the other device converted first');
  ok(await until(() => bs.at(-1) === 'synced', 5000), `L6: this device goes on open (${bs.join(',')})`);
  const ea = fakeEngine();
  ea.setRoomId(raced.id);
  const ac2 = client(ea, a, []);
  await ac2.open();
  ea.add('race after');
  ok(await until(() => eb.ops.has('race after'), 3000), 'L6: and keeps syncing in open mode');
  eb.add('race back');
  ok(await until(() => ea.ops.has('race back'), 3000), 'L6: both ways');
  ok(!bs.includes('error'), `L6: no error (${bw.join(';')})`);
  bc.stop();
  ac2.stop();
}

// ---- guests: an editor syncs both ways, a watcher only follows, a removed guest stops ------------------------
{
  const ANA = 'ana@example.com';
  const nathan = (db.prepare("SELECT id FROM members WHERE email = 'nathan@example.com'").get() as { id: string }).id;
  createInvite(db, nathan, ANA, 'member', Date.now());
  const anaDev = await device(ANA);
  const ana = (db.prepare('SELECT id FROM members WHERE email = ?').get(ANA) as { id: string }).id;
  const asAna = { headers: () => ({ 'x-test-user': ANA }) };

  const ownerDev = await device();
  const mine = fakeEngine();
  mine.add('entrance');
  const os: RoomStatus[] = [];
  const oc = client(mine, ownerDev, os);
  await oc.open();
  ok(await until(() => os.includes('synced')), 'G: the owner is synced');
  const shared = mine.roomId();
  const team = addRoom(db, nathan, shared, 'Team', Date.now());

  // Not a guest yet: Ana's copy of the file gets a room of her own, as any stranger's would.
  ok((await listMyCaves({ base, fetch, device: anaDev, ...asAna, loadKeySeen: async () => null, saveKeySeen: async () => {} })).length === 0, 'G: a stranger lists nothing');

  setGuest(db, nathan, shared, team, ana, 'editor', Date.now());
  const access: ServerAccess = { base, fetch, device: anaDev, ...asAna, loadKeySeen: async () => null, saveKeySeen: async () => {} };
  const listed = await listMyCaves(access);
  ok(listed.length === 1 && listed[0].id === shared && listed[0].role === 'editor' && listed[0].ownerName === 'nathan', `G1: the shared cave is on Ana's list as Nathan's (${JSON.stringify(listed.map((c) => [c.role, c.ownerName]))})`);
  const got = await downloadCave(access, shared);
  ok(got.status === 'ok', 'G1: Ana fetches the shared cave');

  const hers = fakeEngine();
  hers.setRoomId(shared);
  const as: RoomStatus[] = [];
  const roles: Role[] = [];
  const reasons: string[] = [];
  const ac = client(hers, anaDev, as, { ...asAna, onRole: (r) => roles.push(r), reasons });
  await ac.open();
  ok(await until(() => hers.ops.has('entrance')), 'G2: the editor gets the cave');
  ok(roles[0] === 'editor', `G2: and is told she is an editor (${roles.join(',')})`);
  hers.add('her survey');
  ok(await until(() => mine.ops.has('her survey')), 'G2: an editor\'s edit reaches the owner');
  mine.add('his survey');
  ok(await until(() => hers.ops.has('his survey')), 'G2: and the owner\'s reaches her');
  ok(hers.roomId() === shared, 'G2: the guest never makes a room of her own');

  // Made a watcher: the server closes her stream, her client reads the new seat, posts nothing, still follows.
  setGuest(db, nathan, shared, team, ana, 'watcher', Date.now());
  const { closeMember } = await import('../../server/src/streams.ts');
  closeMember(shared, ana);
  ok(await until(() => roles.at(-1) === 'watcher', 6000), `G3: the client hears it is now watching (${roles.join(',')})`);
  const before = rows(shared);
  hers.add('a watcher edit');
  await sleep(300);
  ok(rows(shared) === before, 'G3: a watcher posts nothing');
  mine.add('more from the owner');
  ok(await until(() => hers.ops.has('more from the owner'), 6000), 'G3: a watcher still gets edits');
  ok(!as.includes('error'), `G3: no error for a watcher (${reasons.join(';')})`);

  // Taken off: sync stops with the removal note, and no new room is made of the copy.
  removeGuest(db, nathan, shared, team, ana, Date.now());
  closeMember(shared, ana);
  ok(await until(() => as.at(-1) === 'error', 6000), `G4: a removed guest's sync stops (${as.slice(-3).join(',')})`);
  ok(reasons.at(-1) === REMOVED, `G4: with the removal note (${reasons.at(-1)})`);
  ok(hers.roomId() === shared, 'G4: the copy keeps its room id: no new room');
  ac.stop();

  // Opening the cave again after removal: stops at once with the same note.
  const again: string[] = [];
  const ac2 = client(fakeEngine(), anaDev, [], { ...asAna, reasons: again });
  (ac2 as unknown as { o: { engine: CaveEngine } }).o.engine.setRoomId(shared);
  await ac2.open();
  ok(again.at(-1) === REMOVED, `G5: reopening a cave one was taken off stops with the note (${again.join(';')})`);
  ac2.stop();
  oc.stop();
}

server.close();
console.log(bad ? `roomclient: ${bad} failed` : 'roomclient: all ok');
process.exit(bad ? 1 : 0);
