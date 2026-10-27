// Keeps one open cave in step with its rooms on the CaveCAD server (its owners' devices and its guests').
// Edits are posted; the server numbers them and passes them to the cave's other open streams. Pure: the engine,
// fetch and the device's keys are passed in, so node tests run it against the real server.
//
// Two storage modes, read per cave from `GET /api/caves` (`storage`):
//   - `open` (every new cave): the server keeps plain JSON text. An entry is `{"c": client, "ops": ops}`, a snapshot
//     `{"upTo": n, "ops": ops}` (the inner number must match the server's), a label `{name, summary}`. No keys.
//   - `locked` (caves made during rooms step 1): everything below about keys. A device holding the key converts the
//     cave once it has caught up: it posts an open snapshot at the last number (`POST /api/caves/:id/open`), turns
//     its kept locked entries into open ones with the key still in memory, and syncs open from then on. When another
//     device converted first, it reloads the listing and goes on open. A device with no key waits (`waiting-key`)
//     until the cave is open. The crypto below is used only on this locked/conversion path.
//
// What this client does not take on trust from the server (locked caves):
//   - Entries are unlocked with the author device the server names; a relabelled entry fails to unlock and stops
//     sync with status `error` (it is never applied, and never skipped as "my own").
//   - "My own" is this client, not this device: two tabs on one device share its key and author id, so each client
//     names itself inside the lock (`c`) and skips only entries carrying its own name.
//   - A snapshot's `upTo` is read from inside the lock; the server's plain copy must agree.
//   - The highest key version (and its key id) seen for a cave is remembered; a lower version, or another key under
//     a version already seen, is refused.
// What it does take on trust (step 1): the server's list of this member's devices (see `wrapFor`).
//
// Nothing is lost between opens:
//   - Locked entries not yet posted (`pending`) are kept by the caller per cave (IndexedDB in the app). `stop()`
//     takes the engine's outbox at once and keeps it there too, so an edit made just before a cave switch or a
//     live session survives; the next open of that cave posts them.
//   - Every open starts from the room's snapshot (no entry number is kept between opens: a cave copy can be older
//     or newer than any number kept beside it). The number is kept in memory for reconnects within one open.
//   - Locked entries kept for a cave that turned open, which this device cannot read (no key: it was offline across
//     the conversion), are moved out of `pending` into a quarantine kept per cave (never deleted), and this device's
//     whole copy is sent as one open entry in their place, whatever the synced hash says. The merge is last-writer-wins
//     by stamp, so the offline edits arrive; the user is told once (`QUARANTINE_INFO`), and sync carries on open.
//   - A copy changed while its room was closed (signed out, a live session, a reload that lost the outbox) posts
//     itself whole, once, on the next open. The merge is last-writer-wins by stamp, so re-applying is harmless.
//     Whether it changed is told by a hash of the cave saved when everything local was known to be in the room.
// A stopped client never touches the engine again.
//
// Guests (2026-10-26): the listing says what this member is in the cave (`role`). A watcher's client posts nothing
// (no entries, snapshot or label; its edits stay pending) and only follows the room; the caller locks the engine
// (`onRole`). A seat change closes this member's streams on the server, so every reconnect reads the role again. A
// member taken off every room finds the cave under `gone`: the client stops with `REMOVED` and never makes the cave
// a new room of theirs.
//
// Beside the client, for the Cave Shelf: `listMyCaves` (the member's caves with their labels, so a cave made on
// another device can be listed by name), `downloadCave` (a cave's snapshot and history, to open it on a device that
// does not keep it yet) and `handOverKeys` (locked caves only: wraps each cave key this device has for the member's
// devices that have none).
import { lock, unlock, unwrapCaveKey, wrapCaveKey } from './room-crypto.ts';

export type RoomStatus = 'connecting' | 'synced' | 'offline' | 'waiting-key' | 'error';
/** What the client needs from the cave engine (engine.ts provides it in the app; tests fake it). */
export type CaveEngine = { roomId(): string; setRoomId(id: string): string; outbox(): string; apply(ops: string): number; snapshot(): string; load(ops: string): string };
/** The newest cave key this device has used for a cave: its version and key id. */
export type KeySeen = { version: number; keyId: string };
/** An entry not yet posted: `version` 0 is an open entry (plain text), 1 and up the key version it was locked under. */
export type Pending = { version: number; locked: string };
export type RoomClientOptions = {
  base: string;
  fetch: typeof fetch;
  engine: CaveEngine;
  /** This device: its server id, its private key, and its own public key (base64url, from `makeDeviceKeys`). */
  device: { id: string; privateKey: CryptoKey; publicKey: string };
  /** Extra headers (the desktop bearer); the device header is added here. */
  headers?: () => Record<string, string>;
  /** The newest key seen for a cave (null when none yet), kept by the caller (IndexedDB in the app). */
  loadKeySeen: (cave: string) => Promise<KeySeen | null>;
  saveKeySeen: (cave: string, seen: KeySeen) => Promise<void>;
  /**
   * Takes every entry kept unposted for a cave, by any client on this device (another tab, or this one before a
   * reload): they are removed from the store and become this client's, which keeps them again under its own name.
   */
  loadPending: (cave: string) => Promise<Pending[]>;
  /** Keeps this client's unposted entries for a cave (an empty list removes them). */
  savePending: (cave: string, client: string, entries: Pending[]) => Promise<void>;
  /**
   * Sets aside, for good, locked entries this device can no longer read (the cave turned open while it was offline,
   * and it has no key): added to what is already kept for the cave, never removed by the client.
   */
  saveQuarantine: (cave: string, entries: Pending[]) => Promise<void>;
  /** A hash of the cave as it was when everything in it was known to be in the room (null when none yet). */
  loadSynced: (cave: string) => Promise<string | null>;
  saveSynced: (cave: string, hash: string) => Promise<void>;
  /** The status, and for `error` a short reason (for logs and a tooltip). */
  onStatus: (s: RoomStatus, reason?: string) => void;
  /** The cave's label for cave lists (its name and summary, as text), locked and kept on the server when it changes. */
  label?: () => string | null;
  /** A one-off note for the user (a new room was started for this cave). */
  onInfo?: (message: string) => void;
  /** What this member is in the cave, when it is first read and whenever it changes (a watcher's engine is locked). */
  onRole?: (role: Role) => void;
  /** The cave changed under the views (entries from the room were applied). Called once after catch-up, then per entry. */
  changed?: () => void;
  flushMs?: number;
  snapshotEvery?: number;
  /** How long the cave must be still before its hash is kept as "all in the room". */
  quietMs?: number;
  /** A stream with nothing on it (not even a ping) for this long is dropped and reopened. */
  deadAfterMs?: number;
  reconnectMs?: number;
  openRetryMs?: number;
  keyRetryMs?: number;
  /** The most locked text one entry may carry (the server takes 2 MB). */
  maxEntryBytes?: number;
};

type DeviceInfo = { id: string; publicKey: string };
/** One cave as `GET /api/caves` lists it for this device. */
type ListedCave = {
  id: string;
  keyVersion: number;
  wrapped: string | null;
  createdAt: number;
  label: { keyVersion: number; locked: string } | null;
  needsKeys: boolean;
  storage?: CaveStorage;
  role?: Role;
  ownerName?: string;
};
/** What this member is in a cave: an owner, or a guest with their best seat. */
export type Role = 'owner' | 'editor' | 'watcher';
/** How the server keeps a cave: plain text (`open`) or locked with a cave key (rooms step 1). */
export type CaveStorage = 'open' | 'locked';
const storageOf = (c: ListedCave): CaveStorage => (c.storage === 'open' ? 'open' : 'locked');
/** What talking to the server about this member's caves needs (a room client's options have all of it). */
export type ServerAccess = Pick<RoomClientOptions, 'base' | 'fetch' | 'device' | 'headers' | 'loadKeySeen' | 'saveKeySeen'>;
/**
 * One of the member's caves for the shelf. `hasKey`: it can be opened here now (always, for an open cave). `label` is
 * null until a device sets one (for a locked cave, also when this device has no key).
 */
export type ServerCave = { id: string; createdAt: number; hasKey: boolean; open: boolean; label: string | null; role: Role; ownerName: string };
/** A cave fetched whole from its room: the snapshot's ops (null when the room has none) and each later entry's ops, in order. */
export type Downloaded = { status: 'ok'; snapshot: string | null; entries: string[] } | { status: 'waiting' } | { status: 'unknown' };
type StreamLine = { t: string; number?: number; keyVersion?: number; locked?: string; authorDevice?: string };

const enc = new TextEncoder();
const hex = (n: number) => Array.from(crypto.getRandomValues(new Uint8Array(n)), (b) => b.toString(16).padStart(2, '0')).join('');
const newCaveId = () => 'cave_' + hex(12);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const sha256 = async (s: string) => Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', enc.encode(s))), (b) => b.toString(16).padStart(2, '0')).join('');
const mb = (n: number) => (n / (1024 * 1024)).toFixed(1) + ' MB';

/** Told once when kept locked edits could not be read and this device's whole copy was sent instead. */
export const QUARANTINE_INFO = 'Some edits made offline were re-sent from this device\'s copy of the cave.';
export const NEW_ROOM_INFO = "This cave's room belonged to another account or device; started a new room";
/** Why sync stops for a member taken off every room of the cave. */
export const REMOVED = "You're no longer on this cave's guest list. Your copy stays on this computer but gets no new edits.";
/** Answers on a post that mean "never", not "not now": the entry is kept, and sync stops with `error`. */
const REFUSALS = new Set([400, 403, 404, 409, 410, 413, 422]);

/** Something the server sent (or this device made) that will not be accepted: sync stops with status `error`. */
class Refused extends Error {}
/** The client was stopped while waiting: give up quietly. */
class Stopped extends Error {}
/** The cave's room is this member's, but this device has no key for it yet. */
class Waiting extends Error {}
/** The stream must be read again from the snapshot (the cave just turned open under it). */
class Reconnect extends Error {}
/** A request the server answered with an error status; `code` is its error word. */
export class HttpError extends Error {
  readonly status: number;
  readonly code: string;
  constructor(status: number, code: string) {
    super(code);
    this.status = status;
    this.code = code;
  }
}

/**
 * Splits a JSON array of ops into JSON arrays each small enough that its entry stays within `maxLocked` characters
 * (locked: base64 of the lock; open: the plain text). A single op too large on its own is refused.
 */
export function splitOps(ops: string, maxLocked: number, open = false): string[] {
  const list = JSON.parse(ops) as unknown[];
  // The entry is {"c":"<16 hex>","ops":"<ops escaped>"}; locked, it is base64 of 0x01 | iv (12) | that | tag (16).
  const budget = open ? maxLocked - 64 : Math.floor((maxLocked * 3) / 4) - 29 - 64;
  const out: string[] = [];
  let cur: string[] = [];
  let size = 2;
  for (const op of list) {
    const s = JSON.stringify(op);
    const cost = enc.encode(JSON.stringify(s)).length - 1; // escaped, without its quotes, plus a comma
    if (cost + 2 > budget) throw new Refused(`An edit is too large to sync (${mb(cost)}; the limit is ${mb(maxLocked)})`);
    if (cur.length && size + cost > budget) {
      out.push('[' + cur.join(',') + ']');
      cur = [];
      size = 2;
    }
    cur.push(s);
    size += cost;
  }
  if (cur.length) out.push('[' + cur.join(',') + ']');
  return out;
}

const headersOf = (o: ServerAccess): Record<string, string> => ({ 'Content-Type': 'application/json', 'X-Cave-Device': o.device.id, ...(o.headers?.() ?? {}) });

async function request<T>(o: ServerAccess, method: string, path: string, body?: unknown): Promise<T> {
  const res = await o.fetch(o.base + path, { method, headers: headersOf(o), body: body === undefined ? undefined : JSON.stringify(body), credentials: 'same-origin' });
  let out: (T & { error?: string }) | null = null;
  try {
    out = (await res.json()) as T & { error?: string };
  } catch {
    out = null;
  }
  if (!res.ok) throw new HttpError(res.status, out?.error ?? 'error');
  if (!out) throw new Error('unreadable answer');
  return out;
}

/** The member's caves, and those they were taken off (`gone`). */
const listing = async (o: ServerAccess) => {
  const r = await request<{ caves: ListedCave[]; gone?: string[] }>(o, 'GET', '/api/caves');
  return { caves: r.caves, gone: r.gone ?? [] };
};
const listCaves = async (o: ServerAccess) => (await listing(o)).caves;

/** One JSON request to the member's part of the server, with this device's headers (for the Rooms panel). */
export const callServer = <T>(o: ServerAccess, method: string, path: string, body?: unknown): Promise<T> => request<T>(o, method, path, body);

/**
 * Opens this device's wrapped key for a listed cave (which must have one), checked against the newest key seen for
 * the cave: a lower version, or another key under a version already seen, is refused. Remembers a newer one.
 */
async function openKey(o: ServerAccess, c: ListedCave, opts?: { extractable?: boolean }): Promise<{ key: CryptoKey; id: string }> {
  const seen = await o.loadKeySeen(c.id);
  if (seen && c.keyVersion < seen.version) throw new Refused('key version went backwards');
  let opened;
  try {
    opened = await unwrapCaveKey(c.wrapped!, o.device.privateKey, { cave: c.id, version: c.keyVersion, myPublicKey: o.device.publicKey }, opts);
  } catch {
    throw new Refused('wrapped key does not open');
  }
  if (seen && seen.version === c.keyVersion && seen.keyId !== opened.id) throw new Refused('another key under a version already seen');
  if (!seen || seen.version !== c.keyVersion) await o.saveKeySeen(c.id, { version: c.keyVersion, keyId: opened.id });
  return opened;
}

/** Wraps a cave key for each device; a device whose public key does not parse is skipped. */
async function wrapFor(key: Uint8Array | CryptoKey, devices: DeviceInfo[], ctx: { cave: string; version: number }): Promise<{ device: string; wrapped: string }[]> {
  // Trusts the server's list of this member's devices for now (Nathan's decision); device approval comes later.
  const out: { device: string; wrapped: string }[] = [];
  for (const d of devices) {
    try {
      out.push({ device: d.id, wrapped: await wrapCaveKey(key, d.publicKey, ctx) });
    } catch {
      // a key that does not parse: that device gets no copy
    }
  }
  return out;
}

/** Wraps the cave's key (this device's wrapped copy, opened) for the member's devices that have none. */
async function handKeys(o: ServerAccess, cave: string, version: number, wrappedMine: string): Promise<void> {
  const { devices } = await request<{ devices: DeviceInfo[] }>(o, 'GET', `/api/caves/${cave}/missing`);
  if (!devices.length) return;
  // An extractable copy only for wrapping, dropped when this returns; the everyday key stays non-extractable.
  const { key } = await unwrapCaveKey(wrappedMine, o.device.privateKey, { cave, version, myPublicKey: o.device.publicKey }, { extractable: true });
  const wrapped = await wrapFor(key, devices, { cave, version });
  if (wrapped.length) await request(o, 'POST', `/api/caves/${cave}/keys`, { version, wrapped });
}

const labelCtx = (cave: string, version: number) => ({ cave, version, author: 'label', kind: 'label' }) as const;

/** Unlocks a snapshot (it names no author, so it may be any of this member's devices') and checks its number. */
async function unlockSnapshot(o: ServerAccess, key: CryptoKey, cave: string, snapshot: { upTo: number; keyVersion: number; locked: string }): Promise<{ upTo: number; ops: string }> {
  const { devices } = await request<{ devices: DeviceInfo[] }>(o, 'GET', '/api/devices');
  let body: { upTo?: unknown; ops?: unknown } | null = null;
  for (const d of devices) {
    try {
      body = JSON.parse(await unlock(key, snapshot.locked, { cave, version: snapshot.keyVersion, author: d.id, kind: 'snapshot' }));
      break;
    } catch {
      // not this device's
    }
  }
  if (!body || typeof body.ops !== 'string' || typeof body.upTo !== 'number') throw new Refused('snapshot does not unlock');
  if (body.upTo !== snapshot.upTo) throw new Refused('snapshot number does not match');
  return { upTo: body.upTo, ops: body.ops };
}

/** Unlocks an entry from the stream with the author device the server names; anything else is refused. */
async function unlockEntry(key: CryptoKey, cave: string, version: number, line: StreamLine): Promise<{ c: string; ops: string }> {
  if (line.keyVersion !== version || typeof line.locked !== 'string' || typeof line.authorDevice !== 'string' || !line.authorDevice) throw new Refused('entry malformed');
  let body: { c?: unknown; ops?: unknown };
  try {
    body = JSON.parse(await unlock(key, line.locked, { cave, version, author: line.authorDevice, kind: 'entry' }));
  } catch {
    throw new Refused('entry does not unlock');
  }
  if (typeof body?.c !== 'string' || typeof body.ops !== 'string') throw new Refused('entry malformed');
  return { c: body.c, ops: body.ops };
}

type SnapshotWire = { upTo: number; keyVersion: number; locked: string };

/** Reads an open snapshot: plain `{upTo, ops}` whose number agrees with the server's. */
function readOpenSnapshot(snapshot: SnapshotWire): { upTo: number; ops: string } {
  if (snapshot.keyVersion !== 0) throw new Refused('snapshot is not open');
  let body: { upTo?: unknown; ops?: unknown } | null = null;
  try {
    body = JSON.parse(snapshot.locked);
  } catch {
    body = null;
  }
  if (!body || typeof body.ops !== 'string' || typeof body.upTo !== 'number') throw new Refused('snapshot malformed');
  if (body.upTo !== snapshot.upTo) throw new Refused('snapshot number does not match');
  return { upTo: body.upTo, ops: body.ops };
}

/** Reads an open entry from the stream: plain `{c, ops}`. */
function readOpenEntry(line: StreamLine): { c: string; ops: string } {
  if (line.keyVersion !== 0 || typeof line.locked !== 'string') throw new Refused('entry malformed');
  let body: { c?: unknown; ops?: unknown } | null = null;
  try {
    body = JSON.parse(line.locked);
  } catch {
    body = null;
  }
  if (typeof body?.c !== 'string' || typeof body.ops !== 'string') throw new Refused('entry malformed');
  return { c: body.c, ops: body.ops };
}

/**
 * The member's caves, each with its label (plain for an open cave, unlocked when this device has a locked cave's key). A cave whose key or label
 * cannot be opened is still listed (with no label): opening it says why.
 */
export async function listMyCaves(o: ServerAccess): Promise<ServerCave[]> {
  const out: ServerCave[] = [];
  for (const c of await listCaves(o)) {
    if (storageOf(c) === 'open') {
      out.push({ id: c.id, createdAt: c.createdAt, hasKey: true, open: true, label: c.label?.locked ?? null, role: c.role ?? 'owner', ownerName: c.ownerName ?? '' });
      continue;
    }
    let label: string | null = null;
    if (c.wrapped && c.label && c.label.keyVersion === c.keyVersion) {
      try {
        const { key } = await openKey(o, c);
        label = await unlock(key, c.label.locked, labelCtx(c.id, c.keyVersion));
      } catch {
        label = null;
      }
    }
    out.push({ id: c.id, createdAt: c.createdAt, hasKey: !!c.wrapped, open: false, label, role: c.role ?? 'owner', ownerName: c.ownerName ?? '' });
  }
  return out;
}

/**
 * Wraps every cave key this device has for the member's devices that have none yet, so a new device can open
 * those caves. Errors on one cave do not stop the others. Gives back how many caves were handed on.
 */
export async function handOverKeys(o: ServerAccess): Promise<number> {
  let n = 0;
  for (const c of await listCaves(o)) {
    if (storageOf(c) === 'open' || !c.wrapped || !c.needsKeys) continue;
    try {
      await openKey(o, c); // the same checks as opening the cave: a key that cannot be trusted is not handed on
      await handKeys(o, c.id, c.keyVersion, c.wrapped);
      n++;
    } catch {
      // tried again next time
    }
  }
  return n;
}

/**
 * Fetches a cave whole from its room: the snapshot, then every entry after it up to now (read from the stream
 * until it goes live). `waiting` when this device has no key for it yet, `unknown` when it is not this member's.
 * Throws when the server cannot be reached or something does not unlock.
 */
export async function downloadCave(o: ServerAccess, cave: string, deadAfterMs = 60_000): Promise<Downloaded> {
  const c = (await listCaves(o)).find((x) => x.id === cave);
  if (!c) return { status: 'unknown' };
  const open = storageOf(c) === 'open';
  if (!open && !c.wrapped) return { status: 'waiting' };
  const key = open ? null : (await openKey(o, c)).key;
  const { snapshot } = await request<{ snapshot: SnapshotWire | null }>(o, 'GET', `/api/caves/${cave}/snapshot`);
  let last = 0;
  let snapOps: string | null = null;
  if (snapshot) {
    let s;
    if (open) s = readOpenSnapshot(snapshot);
    else {
      if (snapshot.keyVersion !== c.keyVersion) throw new Refused('snapshot under another key version');
      s = await unlockSnapshot(o, key!, cave, snapshot);
    }
    snapOps = s.ops;
    last = s.upTo;
  }
  const entries: string[] = [];
  const abort = new AbortController();
  let dog: ReturnType<typeof setTimeout> | undefined;
  const pet = () => {
    clearTimeout(dog);
    dog = setTimeout(() => abort.abort(), deadAfterMs);
  };
  pet();
  try {
    const res = await o.fetch(`${o.base}/api/caves/${cave}/stream?after=${last}`, { headers: headersOf(o), credentials: 'same-origin', signal: abort.signal });
    if (!res.ok || !res.body) throw new HttpError(res.status, 'stream');
    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buf = '';
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) throw new Error('the stream ended before catching up');
        pet();
        buf += decoder.decode(value, { stream: true });
        let i;
        while ((i = buf.indexOf('\n')) >= 0) {
          const line = JSON.parse(buf.slice(0, i)) as StreamLine;
          buf = buf.slice(i + 1);
          if (line.t === 'live') return { status: 'ok', snapshot: snapOps, entries };
          if (line.t !== 'entry' || typeof line.number !== 'number' || line.number <= last) continue;
          entries.push((open ? readOpenEntry(line) : await unlockEntry(key!, cave, c.keyVersion, line)).ops);
          last = line.number;
        }
      }
    } finally {
      reader.cancel().catch(() => {});
    }
  } finally {
    clearTimeout(dog);
    abort.abort();
  }
}

export class RoomClient {
  private o: RoomClientOptions;
  /** This client (one per tab and open cave): entries it posted carry it inside the lock, so it skips only its own. */
  private readonly clientId = hex(8);
  private cave = '';
  /** What this member is in the cave (read with the listing on every open and reconnect). */
  private role: Role = 'owner';
  /** How the server keeps this cave: `open` needs no key; `locked` until this client (or another) converts it. */
  private mode: CaveStorage = 'open';
  /** The cave key for locking and unlocking (locked caves only): non-extractable. */
  private key: CryptoKey | null = null;
  /** This device's wrapped copy, kept to unwrap an extractable key only while handing it on. */
  private wrappedMine = '';
  private version = 1;
  /** The last entry applied, in memory only: each open starts from the snapshot, reconnects resume from here. */
  private last = 0;
  private sinceSnapshot = 0;
  /** This client wrote one of the entries since the last snapshot, so it may upload the next (one device does, not all). */
  private authoredSinceSnapshot = false;
  /** The room has no snapshot (its first upload failed): upload one once caught up. */
  private needSnapshot = false;
  private pending: Pending[] = [];
  private pendingLoaded = false;
  /** Whether this copy was checked against the room on this open (and posted whole if it changed while closed). */
  private localChecked = false;
  /** A room id this client set in the cave whose making the server has not yet confirmed; a retry reuses it. */
  private unconfirmed = '';
  /** A conversion to open is under way: nothing is posted meanwhile. */
  private converting = false;
  /** Not before this time is conversion tried again (it failed, or the cave moved on while it was tried). */
  private convertNext = 0;
  /** Turning this client to open mode (its kept entries unlocked): one at a time, whoever noticed first. */
  private switching: Promise<boolean> | null = null;
  /** The stream is being dropped on purpose, to be read again from the snapshot at once. */
  private restart = false;
  /** Restarts since the stream last went live (the second one waits before reading again). */
  private restartsBeforeLive = 0;
  /** The label the server keeps for this cave, as text (null when none, or not readable). */
  private labelKept: string | null = null;
  private infoShown = false;
  private quarantineShown = false;
  private flushing = false;
  private live = false;
  private stopped = false;
  private failed = false;
  private abort: AbortController | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;
  private retry: ReturnType<typeof setTimeout> | null = null;
  private backoff = 0;
  private nextTry = 0;
  /** The cave changed since its hash was last kept, and when it last changed. */
  private dirty = true;
  private lastActivity = 0;
  /** Entries applied during catch-up, waiting for one `changed()` when live. */
  private quietChanges = false;

  constructor(o: RoomClientOptions) {
    this.o = o;
  }

  /** The engine, while running: a stopped client never touches it again. */
  private get eng(): CaveEngine {
    if (this.stopped) throw new Stopped();
    return this.o.engine;
  }
  /** Called after every wait: gives up once stopped. */
  private alive(): void {
    if (this.stopped) throw new Stopped();
  }

  private headers(): Record<string, string> {
    return headersOf(this.o);
  }
  private call<T>(method: string, path: string, body?: unknown): Promise<T> {
    return request<T>(this.o, method, path, body);
  }
  private shown: RoomStatus | null = null;
  /** Reports a status when it changes. */
  private status(s: RoomStatus): void {
    if (this.failed || s === this.shown) return;
    this.shown = s;
    this.o.onStatus(s);
  }
  private later(f: () => void, ms: number): void {
    if (this.stopped) return;
    if (this.retry) clearTimeout(this.retry);
    this.retry = setTimeout(f, ms);
  }
  /** The cave just changed (an edit here, or one applied from the room). */
  private touch(): void {
    this.dirty = true;
    this.lastActivity = Date.now();
  }
  private maxLocked(): number {
    return this.o.maxEntryBytes ?? 1.5 * 1024 * 1024;
  }
  /** The cave can be read and written here: it is open, or this device holds its key. */
  private ready(): boolean {
    return this.mode === 'open' || !!this.key;
  }

  /** Starts keeping the open cave in step. Call once per opened cave; call `stop()` before opening another. */
  async open(): Promise<void> {
    if (this.stopped || this.failed) return;
    if (this.shown !== 'waiting-key') this.status('connecting'); // retries while waiting do not flicker
    try {
      await this.connect();
    } catch (e) {
      if (this.stopped || e instanceof Stopped) return;
      if (e instanceof Refused) return this.fail(e.message);
      if (e instanceof Waiting) {
        this.status('waiting-key');
        return this.later(() => void this.open(), this.o.keyRetryMs ?? 30_000);
      }
      this.status('offline');
      this.later(() => void this.open(), this.o.openRetryMs ?? 15_000);
    }
  }

  private async connect(): Promise<void> {
    if (!this.cave) this.cave = this.eng.roomId();
    if (!this.cave) await this.createRoom();
    else if (this.cave === this.unconfirmed) await this.confirmRoom();
    else {
      const got = await this.loadRoom();
      if (got === 'waiting') throw new Waiting();
      if (got === 'unknown') {
        // Not this member's room at all (another account's file): the content is here, so it gets a room of its own.
        await this.createRoom();
        if (!this.infoShown) {
          this.infoShown = true;
          this.o.onInfo?.(NEW_ROOM_INFO);
        }
      }
    }
    await this.handKeysToMyOtherDevices();
    await this.keepLabel();
    if (!this.pendingLoaded) {
      const kept = await this.o.loadPending(this.cave);
      this.alive();
      this.pendingLoaded = true;
      if (kept.length) {
        this.pending.unshift(...kept);
        await this.savePending();
      }
    }
    // Entries kept in the locked format for a cave that is open now, with no key here to read them: set aside, not
    // dropped, and this copy is sent whole in their place.
    if (this.mode === 'open') {
      const unreadable = this.pending.filter((p) => p.version !== 0);
      if (unreadable.length) {
        await this.setAside(unreadable);
        this.alive();
      }
    }
    if (!this.localChecked) await this.checkLocalCopy();
    this.alive();
    this.timer = setInterval(() => void this.flush(), this.o.flushMs ?? 300);
    void this.listen();
  }

  /** Stops syncing (the cave is closing, or a live session took over). Unposted edits are kept for the next open. */
  stop(): void {
    if (this.stopped) return;
    // Taken now, while the engine still holds this cave: an edit in the last flush window must not be lost.
    let ops = '';
    if (this.ready() && !this.failed && this.cave && this.o.engine.roomId() === this.cave) {
      try {
        ops = this.o.engine.outbox();
      } catch {
        ops = '';
      }
    }
    this.stopped = true;
    this.abort?.abort();
    if (this.timer) clearInterval(this.timer);
    if (this.retry) clearTimeout(this.retry);
    if (ops && ops !== '[]') void this.queue(ops).catch(() => {});
  }

  /** Something could not be trusted or was refused: stop, and say why. */
  private fail(reason: string): void {
    if (this.failed) return;
    this.failed = true;
    this.stop();
    this.shown = 'error';
    this.o.onStatus('error', reason);
  }

  /** Gives the cave a new room id (set in the cave first, so a retry reuses it), then makes the room. */
  private async createRoom(): Promise<void> {
    const id = newCaveId();
    const r = this.eng.setRoomId(id);
    if (r !== 'ok') throw new Refused(`could not set the room id: ${r}`);
    this.cave = id;
    this.unconfirmed = id;
    await this.confirmRoom();
  }

  /** Makes the (open) room for `unconfirmed`; when an earlier try was made but its answer lost, takes it as made. */
  private async confirmRoom(): Promise<void> {
    try {
      await this.call('POST', '/api/caves', { id: this.cave, storage: 'open' });
      this.alive();
    } catch (e) {
      if (!(e instanceof HttpError && e.status === 409)) throw e;
      this.alive();
      // Already made: by an earlier try whose answer was lost (then it is listed as ours), or by no one we know.
      const got = await this.loadRoom();
      if (got === 'ok') {
        this.unconfirmed = '';
        return; // its first snapshot, if missing, is uploaded once caught up
      }
      return this.createRoom();
    }
    this.unconfirmed = '';
    this.mode = 'open';
    this.key = null;
    this.version = 0;
    this.labelKept = null;
    this.eng.outbox(); // the snapshot below carries everything made so far
    await this.putSnapshot(0);
    this.localChecked = true; // the room's first snapshot is this copy
  }

  /**
   * Reads how the server keeps the cave, and for a locked cave opens this device's wrapped key: `unknown` when the
   * cave is not this member's, `waiting` when it is locked and has no key here yet.
   */
  private async loadRoom(): Promise<'ok' | 'waiting' | 'unknown'> {
    const { caves, gone } = await listing(this.o);
    this.alive();
    const mine = caves.find((c) => c.id === this.cave);
    if (!mine && gone.includes(this.cave)) throw new Refused(REMOVED);
    if (!mine) return 'unknown';
    this.setRole(mine.role ?? 'owner');
    if (storageOf(mine) === 'open') {
      this.mode = 'open';
      this.key = null;
      this.wrappedMine = '';
      this.version = 0;
      this.labelKept = mine.label?.locked ?? null;
      return 'ok';
    }
    this.mode = 'locked';
    if (!mine.wrapped) return 'waiting';
    const opened = await openKey(this.o, mine);
    this.alive();
    this.key = opened.key;
    this.wrappedMine = mine.wrapped;
    this.version = mine.keyVersion;
    this.labelKept = null;
    if (mine.label && mine.label.keyVersion === mine.keyVersion) {
      try {
        this.labelKept = await unlock(opened.key, mine.label.locked, labelCtx(this.cave, this.version));
      } catch {
        this.labelKept = null; // replaced below
      }
      this.alive();
    }
    return 'ok';
  }

  private setRole(role: Role): void {
    if (role === this.role && this.roleTold) return;
    this.role = role;
    this.roleTold = true;
    this.o.onRole?.(role);
  }
  private roleTold = false;
  private get watching(): boolean {
    return this.role === 'watcher';
  }

  /** Reads this member's role in the cave again (a dropped stream may mean a seat change); taken off: stops. */
  private async recheckRole(): Promise<void> {
    let got;
    try {
      got = await listing(this.o);
    } catch {
      return; // offline: tried again on the next reconnect
    }
    this.alive();
    const mine = got.caves.find((c) => c.id === this.cave);
    if (mine) this.setRole(mine.role ?? 'owner');
    else if (got.gone.includes(this.cave)) throw new Refused(REMOVED);
  }

  private async handKeysToMyOtherDevices(): Promise<void> {
    if (this.mode === 'locked' && this.wrappedMine) await handKeys(this.o, this.cave, this.version, this.wrappedMine);
    this.alive();
  }

  /** Keeps the cave's label on the server when it differs from the one kept there. Best effort: it only names the cave in lists. */
  private async keepLabel(): Promise<void> {
    try {
      if (!this.o.label || !this.ready() || this.converting || this.watching || this.eng.roomId() !== this.cave) return;
      const text = this.o.label();
      if (!text || text === this.labelKept) return;
      if (this.mode === 'open') await this.call('PUT', `/api/caves/${this.cave}/label`, { locked: text });
      else {
        const locked = await lock(this.key!, text, labelCtx(this.cave, this.version));
        this.alive();
        await this.call('PUT', `/api/caves/${this.cave}/label`, { version: this.version, locked });
      }
      this.labelKept = text;
    } catch {
      // tried again when the cave next goes quiet, or on the next open
    }
  }

  /**
   * Moves locked entries this device cannot read out of `pending` into the quarantine (kept first, then removed), and
   * queues this copy whole as an open entry in their place, whatever the synced hash says. When the quarantine cannot
   * be kept, the entries stay pending and sync stops with `error`.
   */
  private async setAside(unreadable: Pending[]): Promise<void> {
    // The copy is taken first: once stopped this throws, and the entries stay pending for the next open to set aside.
    const local = this.eng.snapshot();
    try {
      await this.o.saveQuarantine(this.cave, unreadable);
    } catch {
      throw new Refused('Edits kept on this device are locked and could not be read or set aside; they are kept here, not synced');
    }
    this.pending = this.pending.filter((p) => !unreadable.includes(p));
    if (local !== '[]') await this.queue(local); // keeps pending too
    else await this.savePending();
    this.localChecked = true;
    if (!this.quarantineShown) {
      this.quarantineShown = true;
      this.o.onInfo?.(QUARANTINE_INFO);
    }
  }

  /** Posts this copy whole when it changed since it was last known to be all in the room (once per open). */
  private async checkLocalCopy(): Promise<void> {
    if (this.watching) return void (this.localChecked = true); // a watcher's copy is the room's, never sent back
    const local = this.eng.snapshot();
    const hash = await sha256(local);
    this.alive();
    const synced = await this.o.loadSynced(this.cave);
    this.alive();
    if (synced !== hash && local !== '[]') await this.queue(local);
    this.localChecked = true;
  }

  private lockCtx(kind: 'entry' | 'snapshot', author: string) {
    return { cave: this.cave, version: this.version, author, kind } as const;
  }

  /** Makes ops (a JSON array) into one or more entries under the size cap (locked for a locked cave) and keeps them. Never touches the engine. */
  private async queue(ops: string): Promise<void> {
    const max = this.maxLocked();
    const open = this.mode === 'open';
    const parts = splitOps(ops, max, open);
    const key = this.key;
    const ctx = this.lockCtx('entry', this.o.device.id);
    for (const p of parts) {
      const plain = JSON.stringify({ c: this.clientId, ops: p });
      const text = open ? plain : await lock(key!, plain, ctx);
      if (text.length > max) throw new Refused(`An edit is too large to sync (${mb(text.length)}; the limit is ${mb(max)})`);
      this.pending.push({ version: open ? 0 : ctx.version, locked: text });
    }
    await this.savePending();
  }

  private async savePending(): Promise<void> {
    await this.o.savePending(this.cave, this.clientId, this.pending.slice()).catch(() => {});
  }

  /** Uploads a whole-cave snapshot covering entries up to `upTo` (the number is inside it too; locked for a locked cave). */
  private async putSnapshot(upTo: number): Promise<void> {
    if (this.watching) return;
    const body = JSON.stringify({ upTo, ops: this.eng.snapshot() });
    if (this.mode === 'open') return void (await this.call('PUT', `/api/caves/${this.cave}/snapshot`, { upTo, locked: body }));
    const locked = await lock(this.key!, body, this.lockCtx('snapshot', this.o.device.id));
    this.alive();
    await this.call('PUT', `/api/caves/${this.cave}/snapshot`, { upTo, version: this.version, locked });
  }

  /**
   * Reads the listing again; when the cave is open now (another device converted it), turns this client open.
   * True when this client is open afterwards.
   */
  private async recheck(): Promise<boolean> {
    if (this.mode === 'open') return (await this.switching) ?? true;
    const mine = (await listCaves(this.o)).find((c) => c.id === this.cave);
    if (!mine || storageOf(mine) !== 'open') return false;
    return this.toOpen();
  }

  /** Turns this client open: kept locked entries are unlocked with the key in memory and kept again as open entries. */
  private toOpen(): Promise<boolean> {
    this.switching ??= (async () => {
      const key = this.key;
      // Open first, so an entry queued meanwhile (a stop) is made open; nothing here waits on `alive()`, so a stop
      // part-way through still leaves every kept entry readable.
      this.mode = 'open';
      this.key = null;
      this.wrappedMine = '';
      this.version = 0;
      this.labelKept = null;
      this.sinceSnapshot = 0;
      this.authoredSinceSnapshot = false;
      this.needSnapshot = false;
      const unreadable: Pending[] = [];
      for (let i = 0; i < this.pending.length; i++) {
        const p = this.pending[i];
        if (p.version === 0) continue;
        let plain = '';
        try {
          if (!key) throw new Error('no key');
          plain = await unlock(key, p.locked, { cave: this.cave, version: p.version, author: this.o.device.id, kind: 'entry' });
          const b = JSON.parse(plain) as { c?: unknown; ops?: unknown };
          if (typeof b?.c !== 'string' || typeof b.ops !== 'string') throw new Error('malformed');
        } catch {
          unreadable.push(p); // set aside below, and this copy sent whole in their place
          continue;
        }
        const at = this.pending.indexOf(p);
        if (at >= 0) this.pending[at] = { version: 0, locked: plain };
      }
      if (unreadable.length) {
        try {
          await this.setAside(unreadable);
        } catch (e) {
          if (this.stopped || e instanceof Stopped) {
            await this.savePending(); // the ones made open are kept so; the rest stay pending for the next open to set aside
            return false;
          }
          this.fail(e instanceof Error ? e.message : 'Edits kept on this device could not be set aside');
          return false;
        }
      } else await this.savePending();
      return true;
    })();
    return this.switching;
  }

  /** Drops the stream on purpose: it is read again from the snapshot at once. */
  private restartStream(): void {
    this.restart = true;
    this.abort?.abort();
  }

  /**
   * A locked cave this device holds the key for, caught up: converts it to open (an open snapshot at the last number,
   * then this client goes open). Another device having converted it first is fine: this one reloads and goes open.
   */
  private async maybeConvert(): Promise<void> {
    if (this.mode !== 'locked' || !this.key || !this.live || this.failed || Date.now() < this.convertNext) return;
    this.converting = true;
    try {
      while (this.flushing) {
        await sleep(10);
        this.alive();
      }
      const upTo = this.last;
      const snapshot = JSON.stringify({ upTo, ops: this.eng.snapshot() });
      try {
        await this.call('POST', `/api/caves/${this.cave}/open`, { upTo, snapshot });
      } catch (e) {
        this.alive();
        // Refused: already open (another device was first), or the cave moved on past `upTo` while this was tried.
        if (e instanceof HttpError && e.status === 400 && (await this.recheck())) throw new Reconnect();
        this.convertNext = Date.now() + 2_000;
        return;
      }
      // Converted: everything up to `upTo` is now this open snapshot, and numbering carries on after it.
      if (!(await this.toOpen())) return;
      this.alive();
    } finally {
      this.converting = false;
    }
    void this.keepLabel();
    void this.flush();
  }

  /** Entries from the room changed the cave: the views hear of it now when live, or once when catch-up ends. */
  private applied(): void {
    this.touch();
    if (this.live) this.o.changed?.();
    else this.quietChanges = true;
  }

  private async catchUpFromSnapshot(): Promise<void> {
    // A reconnect names its last number, so a snapshot it is already past is not downloaded again.
    const q = this.last > 0 ? `?after=${this.last}` : '';
    const { snapshot } = await this.call<{ snapshot: SnapshotWire | null }>('GET', `/api/caves/${this.cave}/snapshot${q}`);
    this.alive();
    if (!snapshot) {
      if (this.last === 0) this.needSnapshot = true;
      return;
    }
    if (this.last > 0 && snapshot.upTo <= this.last) return;
    let body;
    if (this.mode === 'open') body = readOpenSnapshot(snapshot);
    else if (snapshot.keyVersion === 0 && (await this.recheck())) throw new Reconnect(); // converted meanwhile
    else {
      if (snapshot.keyVersion !== this.version) throw new Refused('snapshot under another key version');
      body = await unlockSnapshot(this.o, this.key!, this.cave, snapshot);
    }
    this.alive();
    if (this.last > 0 && body.upTo <= this.last) return;
    this.eng.apply(body.ops);
    this.applied();
    this.last = body.upTo;
    this.sinceSnapshot = 0;
    this.authoredSinceSnapshot = false;
  }

  private async listen(): Promise<void> {
    while (!this.stopped) {
      try {
        await this.catchUpFromSnapshot();
        this.alive();
        const abort = new AbortController();
        this.abort = abort;
        // A stream that goes quiet (no line, not even the server's ping) is dead: drop it and reconnect.
        let dog: ReturnType<typeof setTimeout> | undefined;
        const pet = () => {
          clearTimeout(dog);
          dog = setTimeout(() => abort.abort(), this.o.deadAfterMs ?? 60_000);
        };
        pet();
        try {
          const res = await this.o.fetch(`${this.o.base}/api/caves/${this.cave}/stream?after=${this.last}`, { headers: this.headers(), credentials: 'same-origin', signal: abort.signal });
          this.alive();
          if (!res.ok || !res.body) throw new Error('stream');
          const reader = res.body.getReader();
          const decoder = new TextDecoder();
          let buf = '';
          let read = 0;
          try {
            for (;;) {
              const { value, done } = await reader.read();
              this.alive();
              if (done) break;
              pet();
              buf += decoder.decode(value, { stream: true });
              let i;
              while ((i = buf.indexOf('\n')) >= 0) {
                const line = JSON.parse(buf.slice(0, i)) as StreamLine;
                buf = buf.slice(i + 1);
                await this.onLine(line);
                this.alive();
                // Open entries need no unlocking, so a long catch-up would never give way: let the page (and a stop) in.
                if (++read % 64 === 0) {
                  await sleep(0);
                  this.alive();
                }
              }
              // Caught up on all that was read: a locked cave whose key is here is converted now.
              await this.maybeConvert();
              this.alive();
            }
          } finally {
            reader.cancel().catch(() => {});
          }
        } finally {
          clearTimeout(dog);
        }
      } catch (e) {
        if (this.stopped || e instanceof Stopped) return;
        if (e instanceof Refused) return this.fail(e.message);
        if (e instanceof Reconnect || this.restart) {
          // The cave turned open under this stream: read it again from the open snapshot, now. A second time before
          // going live waits first, so a server that keeps sending such lines is not asked again in a tight loop.
          this.restart = false;
          this.live = false;
          if (this.restartsBeforeLive++) await sleep(this.o.reconnectMs ?? 3_000);
          continue;
        }
      }
      if (this.restart) {
        this.restart = false;
        this.live = false;
        continue;
      }
      // The stream ended, went quiet, or the server dropped a client that fell behind (or this member's seat
      // changed): read the role again, then reconnect after <last>.
      this.live = false;
      try {
        await this.recheckRole();
      } catch (e) {
        if (this.stopped || e instanceof Stopped) return;
        if (e instanceof Refused) return this.fail(e.message);
      }
      this.status('offline');
      await sleep(this.o.reconnectMs ?? 3_000);
    }
  }

  private async onLine(line: StreamLine): Promise<void> {
    if (line.t === 'live') {
      this.live = true;
      this.restartsBeforeLive = 0;
      if (this.quietChanges) {
        this.quietChanges = false;
        this.o.changed?.();
      }
      this.status(this.pending.length ? 'offline' : 'synced');
      if (this.needSnapshot && this.mode === 'open') {
        // (A locked cave gets its open snapshot when it is converted, just after this.)
        this.needSnapshot = false;
        void this.putSnapshot(this.last).catch(() => {}); // still missing on the next open if this fails
      }
      void this.flush();
      return;
    }
    if (line.t !== 'entry' || typeof line.number !== 'number' || line.number <= this.last) return;
    let body;
    // Open: a line under a key version is from before the cave turned open (a stale stream): read again from the snapshot.
    if (this.mode === 'open' && line.keyVersion !== 0) throw new Reconnect();
    if (this.mode === 'open') body = readOpenEntry(line);
    else if (line.keyVersion === 0 && (await this.recheck())) throw new Reconnect(); // another device converted it
    // Unlocked even when it is our own, so an entry relabelled as ours fails here rather than being skipped.
    else body = await unlockEntry(this.key!, this.cave, this.version, line);
    this.alive();
    // Another tab on this device posts with the same author: only this client's own entries are already here.
    if (body.c !== this.clientId) {
      this.eng.apply(body.ops);
      this.applied();
    } else this.authoredSinceSnapshot = true;
    this.last = line.number;
    this.sinceSnapshot++;
    if (this.sinceSnapshot >= (this.o.snapshotEvery ?? 500) && this.authoredSinceSnapshot) {
      this.sinceSnapshot = 0;
      this.authoredSinceSnapshot = false;
      void this.putSnapshot(this.last).catch(() => {});
    }
  }

  /** Not reachable or signed out: try again after a growing wait (300 ms doubling to 30 s). */
  private offline(): void {
    this.backoff = this.backoff ? Math.min(this.backoff * 2, 30_000) : 300;
    this.nextTry = Date.now() + this.backoff;
    this.status('offline');
  }

  /** Posts local edits (locked first, for a locked cave); keeps them in order while offline. */
  async flush(): Promise<void> {
    if (!this.ready() || this.flushing || this.failed || this.stopped || this.converting) return;
    this.flushing = true;
    try {
      const ops = this.eng.outbox();
      const nothingNew = (!ops || ops === '[]') && this.pending.length === 0;
      if (ops && ops !== '[]') {
        this.touch();
        await this.queue(ops);
        this.alive();
      }
      if (Date.now() < this.nextTry) return;
      if (this.watching) {
        // Nothing is posted: edits kept from before the seat changed wait for an editor's seat.
        if (this.live) this.status('synced');
        return;
      }
      while (this.pending.length) {
        const p = this.pending[0];
        try {
          await this.call('POST', `/api/caves/${this.cave}/entries`, p.version === 0 ? { locked: p.locked } : { version: p.version, locked: p.locked });
        } catch (e) {
          // A locked entry refused because the cave turned open (another device converted it): made open, posted again.
          if (e instanceof HttpError && e.status === 400 && p.version !== 0 && (await this.recheck().catch(() => false))) {
            this.alive();
            this.restartStream();
            continue;
          }
          // Made a watcher meanwhile: the edit is kept, and nothing more is posted.
          if (e instanceof HttpError && e.status === 403 && e.code === 'watcher') return this.setRole('watcher');
          if (e instanceof HttpError && REFUSALS.has(e.status)) return this.fail(`The server refused an edit (${e.status} ${e.code}); it is kept on this device`);
          return this.offline(); // 401 (signed out), 5xx, or no network
        }
        this.backoff = 0;
        this.nextTry = 0;
        if (this.pending[0] === p) this.pending.shift();
        await this.savePending();
        this.alive();
      }
      if (this.live) this.status('synced');
      // Nothing new and nothing waited on since the outbox came back empty: all of this copy is in the room.
      if (nothingNew && this.live) await this.rememberSynced();
    } catch (e) {
      if (e instanceof Refused && !this.stopped) this.fail(e.message);
    } finally {
      this.flushing = false;
    }
  }

  /** Keeps a hash of the cave once it has been still for a while, so the next open knows whether it changed. */
  private async rememberSynced(): Promise<void> {
    if (!this.dirty || Date.now() - this.lastActivity < (this.o.quietMs ?? 5_000)) return;
    this.dirty = false;
    const hash = await sha256(this.eng.snapshot());
    await this.o.saveSynced(this.cave, hash).catch(() => {});
    await this.keepLabel(); // the cave was still for a while: a changed name or summary is kept now

  }
}
