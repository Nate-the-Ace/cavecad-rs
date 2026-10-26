// Locks and keys for rooms: device key pairs, wrapped cave keys, locked entries.
import { lock, makeCaveKey, makeDeviceKeys, unlock, unwrapCaveKey, wrapCaveKey } from '../src/lib/room-crypto.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const throws = async (f: () => Promise<unknown>) => { try { await f(); return false; } catch { return true; } };

const laptop = await makeDeviceKeys();
const tablet = await makeDeviceKeys();
ok(/^[A-Za-z0-9_-]{43}$/.test(laptop.publicKey), 'public key is 43 base64url chars');
ok(laptop.privateKey.extractable === false, 'private key cannot be exported');

const cave = await makeCaveKey();
const forTablet = await wrapCaveKey(cave, tablet.publicKey);
const opened = await unwrapCaveKey(forTablet, tablet.privateKey);
const sealed = await lock(opened, 'station A1 to A2: 12.4 m');
ok((await unlock(cave, sealed)) === 'station A1 to A2: 12.4 m', 'the tablet opens what it was given and locks for everyone with the key');
ok(await throws(() => unwrapCaveKey(forTablet, laptop.privateKey)), 'another device cannot open it');

const a = await lock(cave, 'same text');
const b = await lock(cave, 'same text');
ok(a !== b, 'each lock uses a fresh IV');
const flipped = a.slice(0, 20) + (a[20] === 'A' ? 'B' : 'A') + a.slice(21);
ok(await throws(() => unlock(cave, flipped)), 'a changed entry is refused');
ok((await unlock(cave, await lock(cave, 'ünïcödé ✓'))) === 'ünïcödé ✓', 'UTF-8 round trip');

ok((await unlock(cave, await lock(cave, 'x'.repeat(3_000_000)))).length === 3_000_000, 'a 3 MB entry round-trips');

console.log(bad ? `roomcrypto: ${bad} failed` : 'roomcrypto: all ok');
process.exit(bad ? 1 : 0);
