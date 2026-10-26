// Locks and keys for rooms, all on this device (WebCrypto). A device has an X25519 key pair; its private half
// never leaves it. A cave has an AES-256-GCM key; it is handed to a device wrapped with that device's public key
// (an ephemeral X25519 exchange, HKDF, AES-GCM), so the server, which stores the wrapped copies, cannot open them.
// Entries are locked with the cave key and a fresh 12-byte IV. Text forms are base64url.

const enc = new TextEncoder();
const dec = new TextDecoder();
const b64 = (b: ArrayBuffer | Uint8Array) => {
  const u = new Uint8Array(b);
  let s = '';
  for (let i = 0; i < u.length; i += 0x8000) s += String.fromCharCode(...u.subarray(i, i + 0x8000));
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
};
const unb64 = (s: string) => Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), (c) => c.charCodeAt(0));
const INFO = enc.encode('cavecad cave key v1');

export type DeviceKeys = { privateKey: CryptoKey; publicKey: string };

/** A new device key pair: the private key cannot be exported; the public key as base64url. */
export async function makeDeviceKeys(): Promise<DeviceKeys> {
  const pair = (await crypto.subtle.generateKey({ name: 'X25519' }, false, ['deriveBits'])) as CryptoKeyPair;
  return { privateKey: pair.privateKey, publicKey: b64(await crypto.subtle.exportKey('raw', pair.publicKey)) };
}

/** A new cave lock key (extractable, so it can be wrapped for other devices). */
export async function makeCaveKey(): Promise<CryptoKey> {
  return crypto.subtle.generateKey({ name: 'AES-GCM', length: 256 }, true, ['encrypt', 'decrypt']);
}

async function sharedKey(privateKey: CryptoKey, publicRaw: Uint8Array, salt: Uint8Array): Promise<CryptoKey> {
  const pub = await crypto.subtle.importKey('raw', publicRaw, { name: 'X25519' }, false, []);
  const bits = await crypto.subtle.deriveBits({ name: 'X25519', public: pub }, privateKey, 256);
  const hk = await crypto.subtle.importKey('raw', bits, 'HKDF', false, ['deriveKey']);
  return crypto.subtle.deriveKey({ name: 'HKDF', hash: 'SHA-256', salt, info: INFO }, hk, { name: 'AES-GCM', length: 256 }, false, ['encrypt', 'decrypt']);
}

/** Wraps a cave key for the device whose public key is `publicKeyB64`. Format: ephemeral public (32) | iv (12) | sealed key. */
export async function wrapCaveKey(caveKey: CryptoKey, publicKeyB64: string): Promise<string> {
  const eph = (await crypto.subtle.generateKey({ name: 'X25519' }, true, ['deriveBits'])) as CryptoKeyPair;
  const ephPub = new Uint8Array(await crypto.subtle.exportKey('raw', eph.publicKey));
  const k = await sharedKey(eph.privateKey, unb64(publicKeyB64), ephPub);
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const raw = new Uint8Array(await crypto.subtle.exportKey('raw', caveKey));
  const sealed = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, k, raw));
  const out = new Uint8Array(32 + 12 + sealed.length);
  out.set(ephPub, 0);
  out.set(iv, 32);
  out.set(sealed, 44);
  return b64(out);
}

/** Opens a cave key wrapped for this device. Throws when it was wrapped for another device or changed. */
export async function unwrapCaveKey(wrapped: string, privateKey: CryptoKey): Promise<CryptoKey> {
  const bytes = unb64(wrapped);
  const ephPub = bytes.slice(0, 32);
  const k = await sharedKey(privateKey, ephPub, ephPub);
  const raw = await crypto.subtle.decrypt({ name: 'AES-GCM', iv: bytes.slice(32, 44) }, k, bytes.slice(44));
  return crypto.subtle.importKey('raw', raw, { name: 'AES-GCM', length: 256 }, true, ['encrypt', 'decrypt']);
}

/** Locks text with a cave key. Format: iv (12) | ciphertext, base64url. */
export async function lock(caveKey: CryptoKey, text: string): Promise<string> {
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const sealed = new Uint8Array(await crypto.subtle.encrypt({ name: 'AES-GCM', iv }, caveKey, enc.encode(text)));
  const out = new Uint8Array(12 + sealed.length);
  out.set(iv, 0);
  out.set(sealed, 12);
  return b64(out);
}

/** Unlocks text locked with `lock`. Throws when the key is wrong or the entry was changed. */
export async function unlock(caveKey: CryptoKey, locked: string): Promise<string> {
  const bytes = unb64(locked);
  return dec.decode(await crypto.subtle.decrypt({ name: 'AES-GCM', iv: bytes.slice(0, 12) }, caveKey, bytes.slice(12)));
}
