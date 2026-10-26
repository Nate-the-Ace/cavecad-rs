// Locks and keys for rooms, all on this device (WebCrypto). Pure: no storage, no network, importable from node.
//
// Devices. A device has an X25519 key pair; its private half is non-extractable and never leaves it. People
// compare devices by `fingerprint(publicKey)`, a short code shown on both screens.
//
// Cave keys. A cave has an AES-256-GCM key, shaped as `CaveKey = { key, id, raw? }`:
//   - `key` is a CryptoKey for `lock`/`unlock`. It is non-extractable unless asked otherwise, so script that gets
//     hold of it can use it but cannot copy it out.
//   - `id` is `keyId` of the raw key. Computing it needs the raw bytes, so it is computed once, where they exist
//     (in `makeCaveKey` and inside `unwrapCaveKey`), and callers store it beside the key rather than recompute it.
//   - `raw` (the 32 key bytes) is returned only by `makeCaveKey`, so the creating device can wrap the new key for
//     its own and other devices. Keep it in memory only, wrap with it, then drop it.
//   `wrapCaveKey` takes raw bytes or an extractable CryptoKey. A device that later needs to re-wrap a key it was
//   given (adding a member) unwraps it once with `{ extractable: true }`, wraps, and discards that copy; everyday
//   unwraps stay non-extractable.
//
// Wrapping. A cave key is handed to a device wrapped with that device's public key: an ephemeral X25519 exchange
// (ephemeral private half non-extractable), HKDF-SHA256 (salt = ephemeral public key, info =
// "cavecad cave key v1|<ephPub>|<recipientPub>"), AES-GCM with associated data
// "cavecad-wrap-v1|<cave>|<keyVersion>|<recipientPub>". The server stores the wrapped copies and cannot open
// them, nor move one to another cave, key version or device without the unwrap failing.
//   Wrap format: 0x01 | ephemeral public (32) | iv (12) | sealed key (32 + 16 tag).
//
// Entries and snapshots. Text is locked with the cave key, a fresh 12-byte IV and associated data
// "cavecad-<kind>-v1|<cave>|<keyVersion>|<author>", so an entry cannot be replayed as a snapshot, or under another
// cave, key version or author. A cave's label (its name and summary for cave lists) is locked the same way with
// kind `label` and author `label`, since any of the owner's devices may set it.
//   Lock format: 0x01 | iv (12) | ciphertext + tag (16).
//
// Every format starts with a version byte (1); any other version throws. Context strings may not contain "|"
// and key versions are non-negative integers, so the associated data cannot be made ambiguous. Text forms are
// base64url without padding.

const enc = new TextEncoder();
const dec = new TextDecoder();
const FORMAT = 1;
const IV = 12;
const TAG = 16;
const PUB = 32;
const RAW = 32;
const WRAP_LEN = 1 + PUB + IV + RAW + TAG;
type Bytes = Uint8Array<ArrayBuffer>;

const b64 = (b: ArrayBuffer | Uint8Array) => {
  const u = new Uint8Array(b);
  let s = '';
  for (let i = 0; i < u.length; i += 0x8000) s += String.fromCharCode(...u.subarray(i, i + 0x8000));
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
};
const unb64 = (s: string): Bytes => {
  if (typeof s !== 'string' || !/^[A-Za-z0-9_-]*$/.test(s)) throw new Error('room-crypto: not base64url');
  return Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), (c) => c.charCodeAt(0));
};

export type DeviceKeys = { privateKey: CryptoKey; publicKey: string };
export type CaveKey = { key: CryptoKey; id: string; raw?: Uint8Array };
export type WrapContext = { cave: string; version: number };
export type UnwrapContext = { cave: string; version: number; myPublicKey: string };
export type LockKind = 'entry' | 'snapshot' | 'label';
export type LockContext = { cave: string; version: number; author: string; kind: LockKind };

function field(name: string, v: unknown): string {
  if (typeof v !== 'string' || v === '' || v.includes('|')) throw new Error(`room-crypto: bad ${name}`);
  return v;
}
function keyVersion(v: unknown): number {
  if (typeof v !== 'number' || !Number.isSafeInteger(v) || v < 0) throw new Error('room-crypto: bad key version');
  return v;
}
function publicRaw(b64key: string): Bytes {
  const raw = unb64(field('public key', b64key));
  if (raw.length !== PUB) throw new Error('room-crypto: bad public key');
  return raw;
}
function formatOf(bytes: Uint8Array): void {
  if (bytes.length < 1 || bytes[0] !== FORMAT) throw new Error('room-crypto: unknown format version');
}

/** A new device key pair: the private key cannot be exported; the public key as base64url. */
export async function makeDeviceKeys(): Promise<DeviceKeys> {
  const pair = (await crypto.subtle.generateKey({ name: 'X25519' }, false, ['deriveBits'])) as CryptoKeyPair;
  return { privateKey: pair.privateKey, publicKey: b64(await crypto.subtle.exportKey('raw', pair.publicKey)) };
}

/** A short code for comparing a device's public key on two screens: `AB12-CD34-EF56-7890` (SHA-256, first 8 bytes). */
export async function fingerprint(publicKeyB64: string): Promise<string> {
  const h = new Uint8Array(await crypto.subtle.digest('SHA-256', publicRaw(publicKeyB64))).subarray(0, 8);
  const hex = Array.from(h, (x) => x.toString(16).padStart(2, '0')).join('').toUpperCase();
  return hex.match(/.{4}/g)!.join('-');
}

/** A private copy of the raw key bytes; the caller zeroes it when done. */
async function rawOf(caveKey: CryptoKey | Uint8Array): Promise<Bytes> {
  const raw = caveKey instanceof Uint8Array ? new Uint8Array(caveKey) : new Uint8Array(await crypto.subtle.exportKey('raw', caveKey));
  if (raw.length !== RAW) throw new Error('room-crypto: a cave key is 32 bytes');
  return raw;
}

/**
 * A cave key's id: base64url of the first 16 bytes of HKDF-SHA256(raw key, salt empty, info "cavecad key id").
 * Needs the raw key (bytes, or an extractable CryptoKey); a non-extractable key throws. Compute it once at
 * create/unwrap time (both already do, as `CaveKey.id`) and store it.
 */
export async function keyId(caveKey: CryptoKey | Uint8Array): Promise<string> {
  const raw = await rawOf(caveKey);
  const hk = await crypto.subtle.importKey('raw', raw, 'HKDF', false, ['deriveBits']);
  raw.fill(0);
  const bits = await crypto.subtle.deriveBits({ name: 'HKDF', hash: 'SHA-256', salt: new Uint8Array(0), info: enc.encode('cavecad key id') }, hk, 128);
  return b64(bits);
}

async function importCaveKey(raw: Bytes, extractable: boolean): Promise<CryptoKey> {
  return crypto.subtle.importKey('raw', raw, { name: 'AES-GCM', length: 256 }, extractable, ['encrypt', 'decrypt']);
}

/** A new cave key: a non-extractable `key`, its `id`, and the `raw` bytes for wrapping (memory only, then drop). */
export async function makeCaveKey(): Promise<CaveKey & { raw: Uint8Array }> {
  const raw = crypto.getRandomValues(new Uint8Array(RAW));
  return { key: await importCaveKey(raw, false), id: await keyId(raw), raw };
}

async function sharedKey(privateKey: CryptoKey, peerPub: Bytes, ephPub: Bytes, recipientPub: Bytes): Promise<CryptoKey> {
  const pub = await crypto.subtle.importKey('raw', peerPub, { name: 'X25519' }, false, []);
  const bits = await crypto.subtle.deriveBits({ name: 'X25519', public: pub }, privateKey, 256);
  const hk = await crypto.subtle.importKey('raw', bits, 'HKDF', false, ['deriveKey']);
  const info = enc.encode(`cavecad cave key v1|${b64(ephPub)}|${b64(recipientPub)}`);
  return crypto.subtle.deriveKey({ name: 'HKDF', hash: 'SHA-256', salt: ephPub, info }, hk, { name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']);
}

const wrapAad = (cave: string, version: number, recipientPub: Bytes) =>
  enc.encode(`cavecad-wrap-v1|${field('cave', cave)}|${keyVersion(version)}|${b64(recipientPub)}`);

/**
 * Wraps a cave key (raw bytes, or an extractable CryptoKey) for the device whose public key is
 * `recipientPublicKeyB64`, bound to the cave and key version.
 */
export async function wrapCaveKey(caveKey: Uint8Array | CryptoKey, recipientPublicKeyB64: string, ctx: WrapContext): Promise<string> {
  const recipient = publicRaw(recipientPublicKeyB64);
  const aad = wrapAad(ctx.cave, ctx.version, recipient);
  const raw = await rawOf(caveKey);
  const eph = (await crypto.subtle.generateKey({ name: 'X25519' }, false, ['deriveBits'])) as CryptoKeyPair;
  const ephPub = new Uint8Array(await crypto.subtle.exportKey('raw', eph.publicKey));
  const k = await sharedKey(eph.privateKey, recipient, ephPub, recipient);
  const iv = crypto.getRandomValues(new Uint8Array(IV));
  const sealed = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-GCM', iv, additionalData: aad }, k, raw));
  raw.fill(0);
  const out = new Uint8Array(WRAP_LEN);
  out[0] = FORMAT;
  out.set(ephPub, 1);
  out.set(iv, 1 + PUB);
  out.set(sealed, 1 + PUB + IV);
  return b64(out);
}

/**
 * Opens a cave key wrapped for this device. Throws when it was wrapped for another device, cave or key version,
 * or was changed. The key is non-extractable unless `opts.extractable` (only for re-wrapping; drop it after).
 */
export async function unwrapCaveKey(wrapped: string, privateKey: CryptoKey, ctx: UnwrapContext, opts?: { extractable?: boolean }): Promise<CaveKey> {
  const me = publicRaw(ctx.myPublicKey);
  const aad = wrapAad(ctx.cave, ctx.version, me);
  const bytes = unb64(wrapped);
  formatOf(bytes);
  if (bytes.length !== WRAP_LEN) throw new Error('room-crypto: bad wrapped key length');
  const ephPub = bytes.slice(1, 1 + PUB);
  const k = await sharedKey(privateKey, ephPub, ephPub, me);
  const iv = bytes.slice(1 + PUB, 1 + PUB + IV);
  const raw = new Uint8Array(await crypto.subtle.decrypt({ name: 'AES-GCM', iv, additionalData: aad }, k, bytes.slice(1 + PUB + IV)));
  try {
    return { key: await importCaveKey(raw, opts?.extractable === true), id: await keyId(raw) };
  } finally {
    raw.fill(0);
  }
}

const lockAad = (ctx: LockContext) => {
  if (ctx.kind !== 'entry' && ctx.kind !== 'snapshot' && ctx.kind !== 'label') throw new Error('room-crypto: bad kind');
  return enc.encode(`cavecad-${ctx.kind}-v1|${field('cave', ctx.cave)}|${keyVersion(ctx.version)}|${field('author', ctx.author)}`);
};

/** Locks text with a cave key, bound to cave, key version, author and kind. */
export async function lock(caveKey: CryptoKey, text: string, ctx: LockContext): Promise<string> {
  const aad = lockAad(ctx);
  const iv = crypto.getRandomValues(new Uint8Array(IV));
  const sealed = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-GCM', iv, additionalData: aad }, caveKey, enc.encode(text)));
  const out = new Uint8Array(1 + IV + sealed.length);
  out[0] = FORMAT;
  out.set(iv, 1);
  out.set(sealed, 1 + IV);
  return b64(out);
}

/** Unlocks text locked with `lock`. Throws on a wrong key, a changed entry, or any context other than the one it was locked under. */
export async function unlock(caveKey: CryptoKey, locked: string, ctx: LockContext): Promise<string> {
  const aad = lockAad(ctx);
  const bytes = unb64(locked);
  formatOf(bytes);
  if (bytes.length < 1 + IV + TAG) throw new Error('room-crypto: locked text too short');
  const plain = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: bytes.slice(1, 1 + IV), additionalData: aad }, caveKey, bytes.slice(1 + IV));
  return dec.decode(plain);
}
