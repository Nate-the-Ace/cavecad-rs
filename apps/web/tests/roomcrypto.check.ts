// Locks and keys for rooms: device key pairs, wrapped cave keys, locked entries and snapshots, key ids, fingerprints.
import { fingerprint, keyId, lock, makeCaveKey, makeDeviceKeys, unlock, unwrapCaveKey, wrapCaveKey } from '../src/lib/room-crypto.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const throws = async (f: () => Promise<unknown>) => { try { await f(); return false; } catch { return true; } };
const fromB64 = (s: string) => Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), (c) => c.charCodeAt(0));
const toB64 = (u: Uint8Array) => btoa(String.fromCharCode(...u)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
const withByte = (s: string, i: number, v: number) => { const u = fromB64(s); u[i] = v; return toB64(u); };
const flipByte = (s: string, i: number) => { const u = fromB64(s); u[i] ^= 1; return toB64(u); };

// Device keys
const laptop = await makeDeviceKeys();
const tablet = await makeDeviceKeys();
ok(/^[A-Za-z0-9_-]{43}$/.test(laptop.publicKey), 'public key is 43 base64url chars');
ok(laptop.privateKey.extractable === false, 'private key cannot be exported');

// Cave keys: non-extractable for locking, raw bytes only for wrapping
const cave = await makeCaveKey();
ok(cave.key.extractable === false, 'a new cave key cannot be exported');
ok(cave.raw instanceof Uint8Array && cave.raw.length === 32, 'a new cave key comes with its 32 raw bytes');
ok(cave.id === (await keyId(cave.raw)), 'a new cave key carries its key id');

// Wrapping, bound to cave, version and recipient
const wctx = { cave: 'cave-A', version: 1 };
const forTablet = await wrapCaveKey(cave.raw, tablet.publicKey, wctx);
ok(fromB64(forTablet)[0] === 1, 'a wrapped key starts with version byte 1');
const opened = await unwrapCaveKey(forTablet, tablet.privateKey, { ...wctx, myPublicKey: tablet.publicKey });
ok(opened.key.extractable === false, 'an unwrapped key cannot be exported by default');
ok(opened.id === cave.id, 'the unwrapped key has the same key id');
const ectx = { cave: 'cave-A', version: 1, author: 'dev-tablet', kind: 'entry' as const };
const sealed = await lock(opened.key, 'station A1 to A2: 12.4 m', ectx);
ok((await unlock(cave.key, sealed, ectx)) === 'station A1 to A2: 12.4 m', 'the tablet opens what it was given and locks for everyone with the key');
ok(await throws(() => unwrapCaveKey(forTablet, laptop.privateKey, { ...wctx, myPublicKey: laptop.publicKey })), 'another device cannot open it');
ok(await throws(() => unwrapCaveKey(forTablet, tablet.privateKey, { cave: 'cave-B', version: 1, myPublicKey: tablet.publicKey })), 'a wrap for cave A is refused as cave B');
ok(await throws(() => unwrapCaveKey(forTablet, tablet.privateKey, { cave: 'cave-A', version: 2, myPublicKey: tablet.publicKey })), 'a version-1 wrap is refused as version 2');
ok(await throws(() => unwrapCaveKey(forTablet, tablet.privateKey, { ...wctx, myPublicKey: laptop.publicKey })), 'a wrap for device X is refused when myPublicKey is Y');
ok(await throws(() => unwrapCaveKey(forTablet.slice(0, -4), tablet.privateKey, { ...wctx, myPublicKey: tablet.publicKey })), 'a truncated wrap is refused');
ok(await throws(() => unwrapCaveKey(flipByte(forTablet, 60), tablet.privateKey, { ...wctx, myPublicKey: tablet.publicKey })), 'a changed wrap is refused');
ok(await throws(() => unwrapCaveKey(withByte(forTablet, 0, 2), tablet.privateKey, { ...wctx, myPublicKey: tablet.publicKey })), 'a wrap with an unknown version byte is refused');
ok(await throws(() => wrapCaveKey(cave.key, tablet.publicKey, wctx)), 'a non-extractable key cannot be wrapped');

// Re-wrap path: an extractable unwrap can be wrapped onward
const keeper = await unwrapCaveKey(forTablet, tablet.privateKey, { ...wctx, myPublicKey: tablet.publicKey }, { extractable: true });
ok(keeper.key.extractable === true, 'an unwrap asked to be extractable is');
const forLaptop = await wrapCaveKey(keeper.key, laptop.publicKey, wctx);
const onLaptop = await unwrapCaveKey(forLaptop, laptop.privateKey, { ...wctx, myPublicKey: laptop.publicKey });
ok(onLaptop.id === cave.id && (await unlock(onLaptop.key, sealed, ectx)) === 'station A1 to A2: 12.4 m', 'a re-wrapped key opens the same entries');

// Locks, bound to cave, version, author and kind
const a = await lock(cave.key, 'same text', ectx);
const b = await lock(cave.key, 'same text', ectx);
ok(a !== b, 'each lock uses a fresh IV');
ok(fromB64(a)[0] === 1, 'a locked entry starts with version byte 1');
ok(await throws(() => unlock(cave.key, flipByte(a, 20), ectx)), 'a changed entry is refused');
ok(await throws(() => unlock(cave.key, a.slice(0, -3), ectx)), 'a truncated entry is refused');
ok(await throws(() => unlock(cave.key, withByte(a, 0, 7), ectx)), 'an entry with an unknown version byte is refused');
ok(await throws(() => unlock(cave.key, a, { ...ectx, kind: 'snapshot' })), 'an entry is refused as a snapshot');
ok(await throws(() => unlock(cave.key, a, { ...ectx, author: 'dev-laptop' })), 'an entry is refused under another author');
ok(await throws(() => unlock(cave.key, a, { ...ectx, cave: 'cave-B' })), 'an entry is refused under another cave');
ok(await throws(() => unlock(cave.key, a, { ...ectx, version: 2 })), 'an entry is refused under another key version');
const sctx = { ...ectx, kind: 'snapshot' as const };
const snap = await lock(cave.key, 'whole cave', sctx);
ok(fromB64(snap)[0] === 1 && (await unlock(cave.key, snap, sctx)) === 'whole cave', 'a snapshot round-trips with its version byte');
ok(await throws(() => unlock(cave.key, snap, ectx)), 'a snapshot is refused as an entry');
const lctx = { cave: 'cave-A', version: 1, author: 'label', kind: 'label' as const };
const label = await lock(cave.key, '{"name":"Truitt"}', lctx);
ok((await unlock(cave.key, label, lctx)) === '{"name":"Truitt"}', 'a label round-trips');
ok(await throws(() => unlock(cave.key, label, { ...lctx, cave: 'cave-B' })), 'a label is refused under another cave');
ok(await throws(() => unlock(cave.key, label, { ...sctx, author: 'label' })), 'a label is refused as a snapshot');
ok(await throws(() => unlock(cave.key, snap, { ...lctx, author: sctx.author })), 'a snapshot is refused as a label');
ok(await throws(() => lock(cave.key, 'x', { ...ectx, author: 'a|b' })), 'a separator inside a context field is refused');
ok((await unlock(cave.key, await lock(cave.key, 'ünïcödé ✓', ectx), ectx)) === 'ünïcödé ✓', 'UTF-8 round trip');
ok((await unlock(cave.key, await lock(cave.key, 'x'.repeat(3_000_000), sctx), sctx)).length === 3_000_000, 'a 3 MB snapshot round-trips');

// Key ids
const other = await makeCaveKey();
ok(/^[A-Za-z0-9_-]{22}$/.test(cave.id), 'a key id is 16 bytes of base64url');
ok((await keyId(cave.raw)) === (await keyId(keeper.key)), 'a key id is stable for the same key');
ok(other.id !== cave.id, 'different keys have different ids');
ok(await throws(() => keyId(cave.key)), 'a key id needs the raw key');

// Fingerprints
const fp = await fingerprint(laptop.publicKey);
ok(/^[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}-[0-9A-F]{4}$/.test(fp), 'a fingerprint is four groups of four uppercase hex');
ok(fp === (await fingerprint(laptop.publicKey)) && fp !== (await fingerprint(tablet.publicKey)), 'a fingerprint is stable per device and differs between devices');

console.log(bad ? `roomcrypto: ${bad} failed` : 'roomcrypto: all ok');
process.exit(bad ? 1 : 0);
