// Keeps one open cave in step with its room on the CaveCAD server (step 1: the Private room, the owner's own
// devices). Edits are locked here and posted; the server numbers them and passes them to the cave's other open
// streams. Pure: the engine, fetch and the device's keys are passed in, so node tests run it against the real server.
//
// What this client does not take on trust from the server:
//   - Entries are unlocked with the author device the server names; a relabelled entry fails to unlock and stops
//     sync with status `error` (it is never applied, and never skipped as "my own").
//   - "My own" is this client, not this device: two tabs on one device share its key and author id, so each client
//     names itself inside the lock (`c`) and skips only entries carrying its own name.
//   - A snapshot's `upTo` is read from inside the lock; the server's plain copy must agree.
//   - The highest key version (and its key id) seen for a cave is remembered; a lower version, or another key under
//     a version already seen, is refused.
// What it does take on trust (step 1): the server's list of this member's devices (see `wrapForDevices`).
//
// Nothing is lost between opens:
//   - Locked entries not yet posted (`pending`) are kept by the caller per cave (IndexedDB in the app). `stop()`
//     takes the engine's outbox at once and keeps it there too, so an edit made just before a cave switch or a
//     live session survives; the next open of that cave posts them.
//   - Every open starts from the room's snapshot (no entry number is kept between opens: a cave copy can be older
//     or newer than any number kept beside it). The number is kept in memory for reconnects within one open.
//   - A copy changed while its room was closed (signed out, a live session, a reload that lost the outbox) posts
//     itself whole, once, on the next open. The merge is last-writer-wins by stamp, so re-applying is harmless.
//     Whether it changed is told by a hash of the cave saved when everything local was known to be in the room.
// A stopped client never touches the engine again.
import { lock, makeCaveKey, unlock, unwrapCaveKey, wrapCaveKey } from './room-crypto.ts';

export type RoomStatus = 'connecting' | 'synced' | 'offline' | 'waiting-key' | 'error';
/** What the client needs from the cave engine (engine.ts provides it in the app; tests fake it). */
export type CaveEngine = { roomId(): string; setRoomId(id: string): string; outbox(): string; apply(ops: string): number; snapshot(): string; load(ops: string): string };
/** The newest cave key this device has used for a cave: its version and key id. */
export type KeySeen = { version: number; keyId: string };
/** A locked entry not yet posted, with the key version it was locked under. */
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
  /** A hash of the cave as it was when everything in it was known to be in the room (null when none yet). */
  loadSynced: (cave: string) => Promise<string | null>;
  saveSynced: (cave: string, hash: string) => Promise<void>;
  /** The status, and for `error` a short reason (for logs and a tooltip). */
  onStatus: (s: RoomStatus, reason?: string) => void;
  /** A one-off note for the user (a new room was started for this cave). */
  onInfo?: (message: string) => void;
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
type StreamLine = { t: string; number?: number; keyVersion?: number; locked?: string; authorDevice?: string };

const enc = new TextEncoder();
const hex = (n: number) => Array.from(crypto.getRandomValues(new Uint8Array(n)), (b) => b.toString(16).padStart(2, '0')).join('');
const newCaveId = () => 'cave_' + hex(12);
const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const sha256 = async (s: string) => Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', enc.encode(s))), (b) => b.toString(16).padStart(2, '0')).join('');
const mb = (n: number) => (n / (1024 * 1024)).toFixed(1) + ' MB';

export const NEW_ROOM_INFO = "This cave's room belonged to another account or device; started a new room";
/** Answers on a post that mean "never", not "not now": the entry is kept, and sync stops with `error`. */
const REFUSALS = new Set([400, 403, 404, 409, 410, 413, 422]);

/** Something the server sent (or this device made) that will not be accepted: sync stops with status `error`. */
class Refused extends Error {}
/** The client was stopped while waiting: give up quietly. */
class Stopped extends Error {}
/** The cave's room is this member's, but this device has no key for it yet. */
class Waiting extends Error {}
class HttpError extends Error {
  readonly status: number;
  readonly code: string;
  constructor(status: number, code: string) {
    super(code);
    this.status = status;
    this.code = code;
  }
}

/**
 * Splits a JSON array of ops into JSON arrays each small enough that its locked entry stays within `maxLocked`
 * characters. A single op too large on its own is refused.
 */
export function splitOps(ops: string, maxLocked: number): string[] {
  const list = JSON.parse(ops) as unknown[];
  // Locked text is base64 of 0x01 | iv (12) | plain | tag (16); plain is {"c":"<16 hex>","ops":"<ops escaped>"}.
  const budget = Math.floor((maxLocked * 3) / 4) - 29 - 64;
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

export class RoomClient {
  private o: RoomClientOptions;
  /** This client (one per tab and open cave): entries it posted carry it inside the lock, so it skips only its own. */
  private readonly clientId = hex(8);
  private cave = '';
  /** The cave key for locking and unlocking: non-extractable. */
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
  private made: { key: CryptoKey; id: string; raw: Uint8Array } | null = null;
  private infoShown = false;
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
    return { 'Content-Type': 'application/json', 'X-Cave-Device': this.o.device.id, ...(this.o.headers?.() ?? {}) };
  }
  private async call<T>(method: string, path: string, body?: unknown): Promise<T> {
    const res = await this.o.fetch(this.o.base + path, { method, headers: this.headers(), body: body === undefined ? undefined : JSON.stringify(body), credentials: 'same-origin' });
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
      const got = await this.loadKey();
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
    if (!this.pendingLoaded) {
      const kept = await this.o.loadPending(this.cave);
      this.alive();
      this.pendingLoaded = true;
      if (kept.length) {
        this.pending.unshift(...kept);
        await this.savePending();
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
    if (this.key && !this.failed && this.cave && this.o.engine.roomId() === this.cave) {
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

  /** Wraps the cave key for each device; a device whose public key does not parse is skipped. */
  private async wrapForDevices(key: Uint8Array | CryptoKey, devices: DeviceInfo[]): Promise<{ device: string; wrapped: string }[]> {
    // Trusts the server's list of this member's devices for now (Nathan's decision); device approval comes later.
    const out: { device: string; wrapped: string }[] = [];
    for (const d of devices) {
      try {
        out.push({ device: d.id, wrapped: await wrapCaveKey(key, d.publicKey, { cave: this.cave, version: this.version }) });
      } catch {
        // a key that does not parse: that device gets no copy
      }
      this.alive();
    }
    return out;
  }

  /** Gives the cave a new room id (set in the cave first, so a retry reuses it), then makes the room. */
  private async createRoom(): Promise<void> {
    const id = newCaveId();
    const r = this.eng.setRoomId(id);
    if (r !== 'ok') throw new Refused(`could not set the room id: ${r}`);
    this.cave = id;
    this.unconfirmed = id;
    this.version = 1;
    this.made = null;
    await this.confirmRoom();
  }

  /** Makes the room for `unconfirmed`; when an earlier try was made but its answer lost, takes it as made. */
  private async confirmRoom(): Promise<void> {
    if (!this.made) this.made = await makeCaveKey();
    const made = this.made;
    this.alive();
    try {
      const { devices } = await this.call<{ devices: DeviceInfo[] }>('GET', '/api/devices');
      this.alive();
      await this.call('POST', '/api/caves', { id: this.cave, wrapped: await this.wrapForDevices(made.raw, devices) });
      this.alive();
    } catch (e) {
      if (!(e instanceof HttpError && e.status === 409)) throw e;
      this.alive();
      // Already made: by an earlier try whose answer was lost (then it lists our wrapped key), or by no one we know.
      const got = await this.loadKey();
      made.raw.fill(0);
      this.made = null;
      if (got === 'ok') {
        this.unconfirmed = '';
        return; // its first snapshot, if missing, is uploaded once caught up
      }
      return this.createRoom();
    }
    this.made = null;
    made.raw.fill(0);
    this.unconfirmed = '';
    this.key = made.key;
    await this.o.saveKeySeen(this.cave, { version: 1, keyId: made.id });
    this.alive();
    this.eng.outbox(); // the snapshot below carries everything made so far
    await this.putSnapshot(0);
    this.localChecked = true; // the room's first snapshot is this copy
  }

  /** Opens this device's wrapped key: `unknown` when the cave is not this member's, `waiting` when it has no key here yet. */
  private async loadKey(): Promise<'ok' | 'waiting' | 'unknown'> {
    const { caves } = await this.call<{ caves: { id: string; keyVersion: number; wrapped: string | null }[] }>('GET', '/api/caves');
    this.alive();
    const mine = caves.find((c) => c.id === this.cave);
    if (!mine) return 'unknown';
    if (!mine.wrapped) return 'waiting';
    const seen = await this.o.loadKeySeen(this.cave);
    this.alive();
    if (seen && mine.keyVersion < seen.version) throw new Refused('key version went backwards');
    let opened;
    try {
      opened = await unwrapCaveKey(mine.wrapped, this.o.device.privateKey, { cave: this.cave, version: mine.keyVersion, myPublicKey: this.o.device.publicKey });
    } catch {
      throw new Refused('wrapped key does not open');
    }
    this.alive();
    if (seen && seen.version === mine.keyVersion && seen.keyId !== opened.id) throw new Refused('another key under a version already seen');
    this.key = opened.key;
    this.wrappedMine = mine.wrapped;
    this.version = mine.keyVersion;
    if (!seen || seen.version !== this.version) await this.o.saveKeySeen(this.cave, { version: this.version, keyId: opened.id });
    this.alive();
    return 'ok';
  }

  private async handKeysToMyOtherDevices(): Promise<void> {
    const { devices } = await this.call<{ devices: DeviceInfo[] }>('GET', `/api/caves/${this.cave}/missing`);
    this.alive();
    if (!devices.length || !this.wrappedMine) return;
    // An extractable copy only for wrapping, dropped when this returns; the everyday key stays non-extractable.
    const { key } = await unwrapCaveKey(this.wrappedMine, this.o.device.privateKey, { cave: this.cave, version: this.version, myPublicKey: this.o.device.publicKey }, { extractable: true });
    this.alive();
    const wrapped = await this.wrapForDevices(key, devices);
    if (wrapped.length) await this.call('POST', `/api/caves/${this.cave}/keys`, { version: this.version, wrapped });
    this.alive();
  }

  /** Posts this copy whole when it changed since it was last known to be all in the room (once per open). */
  private async checkLocalCopy(): Promise<void> {
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

  /** Locks ops (a JSON array) as one or more entries under the size cap and keeps them. Never touches the engine. */
  private async queue(ops: string): Promise<void> {
    const max = this.maxLocked();
    const parts = splitOps(ops, max);
    const key = this.key!;
    const ctx = this.lockCtx('entry', this.o.device.id);
    for (const p of parts) {
      const locked = await lock(key, JSON.stringify({ c: this.clientId, ops: p }), ctx);
      if (locked.length > max) throw new Refused(`An edit is too large to sync (${mb(locked.length)}; the limit is ${mb(max)})`);
      this.pending.push({ version: ctx.version, locked });
    }
    await this.savePending();
  }

  private async savePending(): Promise<void> {
    await this.o.savePending(this.cave, this.clientId, this.pending.slice()).catch(() => {});
  }

  /** Uploads a locked whole-cave snapshot covering entries up to `upTo` (the number is inside the lock too). */
  private async putSnapshot(upTo: number): Promise<void> {
    const body = JSON.stringify({ upTo, ops: this.eng.snapshot() });
    const locked = await lock(this.key!, body, this.lockCtx('snapshot', this.o.device.id));
    this.alive();
    await this.call('PUT', `/api/caves/${this.cave}/snapshot`, { upTo, version: this.version, locked });
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
    const { snapshot } = await this.call<{ snapshot: { upTo: number; keyVersion: number; locked: string } | null }>('GET', `/api/caves/${this.cave}/snapshot${q}`);
    this.alive();
    if (!snapshot) {
      if (this.last === 0) this.needSnapshot = true;
      return;
    }
    if (this.last > 0 && snapshot.upTo <= this.last) return;
    if (snapshot.keyVersion !== this.version) throw new Refused('snapshot under another key version');
    // The snapshot names no author, so it may be from any of this member's devices.
    const { devices } = await this.call<{ devices: DeviceInfo[] }>('GET', '/api/devices');
    this.alive();
    let body: { upTo?: unknown; ops?: unknown } | null = null;
    for (const d of devices) {
      try {
        body = JSON.parse(await unlock(this.key!, snapshot.locked, this.lockCtx('snapshot', d.id)));
        break;
      } catch {
        // not this device's
      }
    }
    this.alive();
    if (!body || typeof body.ops !== 'string' || typeof body.upTo !== 'number') throw new Refused('snapshot does not unlock');
    if (body.upTo !== snapshot.upTo) throw new Refused('snapshot number does not match');
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
              }
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
      }
      // The stream ended, went quiet, or the server dropped a client that fell behind: reconnect after <last>.
      this.live = false;
      this.status('offline');
      await sleep(this.o.reconnectMs ?? 3_000);
    }
  }

  private async onLine(line: StreamLine): Promise<void> {
    if (line.t === 'live') {
      this.live = true;
      if (this.quietChanges) {
        this.quietChanges = false;
        this.o.changed?.();
      }
      this.status(this.pending.length ? 'offline' : 'synced');
      if (this.needSnapshot) {
        this.needSnapshot = false;
        void this.putSnapshot(this.last).catch(() => {}); // still missing on the next open if this fails
      }
      void this.flush();
      return;
    }
    if (line.t !== 'entry' || typeof line.number !== 'number' || line.number <= this.last) return;
    if (line.keyVersion !== this.version || typeof line.locked !== 'string' || typeof line.authorDevice !== 'string' || !line.authorDevice) throw new Refused('entry malformed');
    // Unlocked even when it is our own, so an entry relabelled as ours fails here rather than being skipped.
    let body: { c?: unknown; ops?: unknown };
    try {
      body = JSON.parse(await unlock(this.key!, line.locked, this.lockCtx('entry', line.authorDevice)));
    } catch {
      throw new Refused('entry does not unlock');
    }
    this.alive();
    if (typeof body?.c !== 'string' || typeof body.ops !== 'string') throw new Refused('entry malformed');
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

  /** Locks and posts local edits; keeps them in order while offline. */
  async flush(): Promise<void> {
    if (!this.key || this.flushing || this.failed || this.stopped) return;
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
      while (this.pending.length) {
        const p = this.pending[0];
        try {
          await this.call('POST', `/api/caves/${this.cave}/entries`, { version: p.version, locked: p.locked });
        } catch (e) {
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
  }
}
