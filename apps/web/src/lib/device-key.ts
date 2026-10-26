// This device's room key pair. Made once, the private half kept as a non-extractable CryptoKey in IndexedDB
// (it never leaves the device), the public half registered with the server, which gives back a device id.
// Beside it, per cave (see room-client.ts): the newest cave key seen, locked entries not yet posted (per client),
// and a hash of the cave as it was when all of it was known to be in the room.
import { makeDeviceKeys } from './room-crypto';
import type { KeySeen, Pending } from './room-client';

/** This device as the room client needs it. */
export type Device = { id: string; privateKey: CryptoKey; publicKey: string };
type Stored = { privateKey: CryptoKey; publicKey: string; deviceId: string };
const DB = 'cavecad-device';

function idb<T>(mode: IDBTransactionMode, f: (s: IDBObjectStore) => IDBRequest): Promise<T | undefined> {
  return new Promise((resolve) => {
    let r: IDBOpenDBRequest;
    try {
      r = indexedDB.open(DB, 1);
    } catch {
      return resolve(undefined);
    }
    r.onupgradeneeded = () => r.result.createObjectStore('kv');
    r.onerror = () => resolve(undefined);
    r.onsuccess = () => {
      try {
        const q = f(r.result.transaction('kv', mode).objectStore('kv'));
        q.onsuccess = () => (resolve(q.result as T), r.result.close());
        q.onerror = () => (resolve(undefined), r.result.close());
      } catch {
        resolve(undefined);
        r.result.close();
      }
    };
  });
}

/** Takes (reads and removes) every value whose key starts with `prefix`, in key order. */
function idbTake<T>(prefix: string): Promise<T[]> {
  return new Promise((resolve) => {
    let r: IDBOpenDBRequest;
    try {
      r = indexedDB.open(DB, 1);
    } catch {
      return resolve([]);
    }
    r.onupgradeneeded = () => r.result.createObjectStore('kv');
    r.onerror = () => resolve([]);
    r.onsuccess = () => {
      const out: T[] = [];
      try {
        const tx = r.result.transaction('kv', 'readwrite');
        const q = tx.objectStore('kv').openCursor(IDBKeyRange.bound(prefix, prefix + '\uffff'));
        q.onsuccess = () => {
          const c = q.result;
          if (!c) return;
          out.push(c.value as T);
          c.delete();
          c.continue();
        };
        tx.oncomplete = () => (resolve(out), r.result.close());
        tx.onerror = tx.onabort = () => (resolve([]), r.result.close());
      } catch {
        resolve([]);
        r.result.close();
      }
    };
  });
}

/** One pending look-up per member signed in on this browser (a device id belongs to one member). */
const cached = new Map<string, Promise<Device | null>>();

/**
 * This device's id and keys for `member`, registering it with the server the first time. Null when the server
 * can't be reached yet (the key pair is kept, and registering is tried again on the next call).
 */
export function deviceKey(base: string, member: string, headers: () => Record<string, string>): Promise<Device | null> {
  const slot = 'keys:' + member;
  let p = cached.get(member);
  if (p) return p;
  p = (async () => {
    let s = await idb<Stored>('readonly', (st) => st.get(slot));
    if (!s) {
      const k = await makeDeviceKeys();
      s = { privateKey: k.privateKey, publicKey: k.publicKey, deviceId: '' };
    }
    if (!s.deviceId) {
      try {
        const res = await fetch(base + '/api/devices', {
          method: 'POST',
          headers: { 'Content-Type': 'application/json', ...headers() },
          body: JSON.stringify({ publicKey: s.publicKey }),
          credentials: 'same-origin',
        });
        if (!res.ok) throw new Error(String(res.status));
        s.deviceId = (await res.json()).device.id;
      } catch {
        const keep = s;
        await idb('readwrite', (st) => st.put(keep, slot));
        cached.delete(member);
        return null;
      }
    }
    const keep = s;
    await idb('readwrite', (st) => st.put(keep, slot));
    return { id: keep.deviceId, privateKey: keep.privateKey, publicKey: keep.publicKey };
  })();
  cached.set(member, p);
  return p;
}

/** Locked entries not yet posted, per cave and client (a tab): `load` takes every client's for the cave. */
export const pendingEntries = {
  load: async (cave: string): Promise<Pending[]> => (await idbTake<Pending[]>(`pending:${cave}|`)).flat(),
  save: async (cave: string, client: string, entries: Pending[]): Promise<void> =>
    void (await idb('readwrite', (st) => (entries.length ? st.put(entries.map((e) => ({ version: e.version, locked: e.locked })), `pending:${cave}|${client}`) : st.delete(`pending:${cave}|${client}`)))),
};

/** A hash of each cave as it was when all of it was known to be in its room. */
export const syncedHash = {
  load: async (cave: string): Promise<string | null> => (await idb<string>('readonly', (st) => st.get('synced:' + cave))) ?? null,
  save: async (cave: string, hash: string): Promise<void> => void (await idb('readwrite', (st) => st.put(hash, 'synced:' + cave))),
};

/** The newest cave key seen per cave (its version and key id). */
export const keySeen = {
  load: async (cave: string): Promise<KeySeen | null> => (await idb<KeySeen>('readonly', (st) => st.get('keyseen:' + cave))) ?? null,
  save: async (cave: string, seen: KeySeen): Promise<void> => void (await idb('readwrite', (st) => st.put({ version: seen.version, keyId: seen.keyId }, 'keyseen:' + cave))),
};
