// Keeps one open cave in step with its room on the CaveCAD server (step 1: the Private room, the owner's own
// devices). Edits are locked here and posted; the server numbers them and passes them to the cave's other open
// streams. Pure: the engine, fetch and the device's keys are passed in, so node tests run it against the real server.
//
// What this client does not take on trust from the server:
//   - Entries are unlocked with the author device the server names; a relabelled entry fails to unlock and stops
//     sync with status `error` (it is never applied, and never skipped as "my own").
//   - A snapshot's `upTo` is read from inside the lock; the server's plain copy must agree.
//   - The highest key version (and its key id) seen for a cave is remembered; a lower version, or another key under
//     a version already seen, is refused.
// What it does take on trust (step 1): the server's list of this member's devices (see `wrapForDevices`).
import { lock, makeCaveKey, unlock, unwrapCaveKey, wrapCaveKey } from './room-crypto.ts';

export type RoomStatus = 'connecting' | 'synced' | 'offline' | 'waiting-key' | 'error';
/** What the client needs from the cave engine (engine.ts provides it in the app; tests fake it). */
export type CaveEngine = { roomId(): string; setRoomId(id: string): string; outbox(): string; apply(ops: string): number; snapshot(): string; load(ops: string): string };
/** The newest cave key this device has used for a cave: its version and key id. */
export type KeySeen = { version: number; keyId: string };
export type RoomClientOptions = {
  base: string;
  fetch: typeof fetch;
  engine: CaveEngine;
  /** This device: its server id, its private key, and its own public key (base64url, from `makeDeviceKeys`). */
  device: { id: string; privateKey: CryptoKey; publicKey: string };
  /** Extra headers (the desktop bearer); the device header is added here. */
  headers?: () => Record<string, string>;
  /** Last applied entry number for a cave, kept by the caller (IndexedDB in the app). */
  loadLast: (cave: string) => Promise<number>;
  saveLast: (cave: string, n: number) => Promise<void>;
  /** The newest key seen for a cave (null when none yet), kept by the caller beside the last number. */
  loadKeySeen: (cave: string) => Promise<KeySeen | null>;
  saveKeySeen: (cave: string, seen: KeySeen) => Promise<void>;
  onStatus: (s: RoomStatus) => void;
  flushMs?: number;
  snapshotEvery?: number;
};

type DeviceInfo = { id: string; publicKey: string };
type StreamLine = { t: string; number?: number; keyVersion?: number; locked?: string; authorDevice?: string };

const newCaveId = () => 'cave_' + Array.from(crypto.getRandomValues(new Uint8Array(12)), (b) => b.toString(16).padStart(2, '0')).join('');

/** Something the server sent that this device will not accept: sync stops with status `error`. */
class Refused extends Error {}

export class RoomClient {
  private o: RoomClientOptions;
  private cave = '';
  /** The cave key for locking and unlocking: non-extractable. */
  private key: CryptoKey | null = null;
  /** This device's wrapped copy, kept to unwrap an extractable key only while handing it on. */
  private wrappedMine = '';
  private version = 1;
  private last = 0;
  private sinceSnapshot = 0;
  private pending: string[] = [];
  private flushing = false;
  private live = false;
  private stopped = false;
  private failed = false;
  private abort: AbortController | null = null;
  private timer: ReturnType<typeof setInterval> | null = null;
  private retry: ReturnType<typeof setTimeout> | null = null;

  constructor(o: RoomClientOptions) {
    this.o = o;
  }

  private headers(): Record<string, string> {
    return { 'Content-Type': 'application/json', 'X-Cave-Device': this.o.device.id, ...(this.o.headers?.() ?? {}) };
  }
  private async call<T>(method: string, path: string, body?: unknown): Promise<T> {
    const res = await this.o.fetch(this.o.base + path, { method, headers: this.headers(), body: body === undefined ? undefined : JSON.stringify(body), credentials: 'same-origin' });
    const out = (await res.json()) as T & { error?: string };
    if (!res.ok) throw new Error(out.error ?? 'error');
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

  /** Starts keeping the open cave in step. Safe to call once per opened cave; call `stop()` before opening another. */
  async open(): Promise<void> {
    if (this.stopped || this.failed) return;
    this.status('connecting');
    try {
      this.cave = this.o.engine.roomId();
      if (!this.cave) await this.createRoom();
      else if (!(await this.loadKey())) return this.waitForKey();
      await this.handKeysToMyOtherDevices();
      this.last = await this.o.loadLast(this.cave);
      if (this.stopped) return;
      this.timer = setInterval(() => void this.flush(), this.o.flushMs ?? 300);
      void this.listen();
    } catch (e) {
      if (e instanceof Refused) return this.fail();
      this.status('offline');
      this.later(() => void this.open(), 15_000);
    }
  }

  /** Stops syncing (the cave is closing). */
  stop(): void {
    this.stopped = true;
    this.abort?.abort();
    if (this.timer) clearInterval(this.timer);
    if (this.retry) clearTimeout(this.retry);
  }

  /** Something could not be trusted: stop, and say so. */
  private fail(): void {
    this.stop();
    if (this.failed) return;
    this.o.onStatus('error');
    this.failed = true;
  }

  /** Wraps the cave key for each device. */
  private async wrapForDevices(key: Uint8Array | CryptoKey, devices: DeviceInfo[]): Promise<{ device: string; wrapped: string }[]> {
    // Trusts the server's list of this member's devices for now (Nathan's decision); device approval comes later.
    return Promise.all(devices.map(async (d) => ({ device: d.id, wrapped: await wrapCaveKey(key, d.publicKey, { cave: this.cave, version: this.version }) })));
  }

  private async createRoom(): Promise<void> {
    this.cave = newCaveId();
    this.version = 1;
    const made = await makeCaveKey();
    try {
      const { devices } = await this.call<{ devices: DeviceInfo[] }>('GET', '/api/devices');
      await this.call('POST', '/api/caves', { id: this.cave, wrapped: await this.wrapForDevices(made.raw, devices) });
    } finally {
      made.raw.fill(0);
    }
    const r = this.o.engine.setRoomId(this.cave);
    if (r !== 'ok') throw new Error(r);
    this.key = made.key;
    await this.o.saveKeySeen(this.cave, { version: 1, keyId: made.id });
    this.o.engine.outbox(); // the snapshot below carries everything made so far
    await this.putSnapshot(0);
  }

  private async loadKey(): Promise<boolean> {
    const { caves } = await this.call<{ caves: { id: string; keyVersion: number; wrapped: string | null }[] }>('GET', '/api/caves');
    const mine = caves.find((c) => c.id === this.cave);
    if (!mine || !mine.wrapped) return false;
    const seen = await this.o.loadKeySeen(this.cave);
    if (seen && mine.keyVersion < seen.version) throw new Refused('key version went backwards');
    let opened;
    try {
      opened = await unwrapCaveKey(mine.wrapped, this.o.device.privateKey, { cave: this.cave, version: mine.keyVersion, myPublicKey: this.o.device.publicKey });
    } catch {
      throw new Refused('wrapped key does not open');
    }
    if (seen && seen.version === mine.keyVersion && seen.keyId !== opened.id) throw new Refused('another key under a version already seen');
    this.key = opened.key;
    this.wrappedMine = mine.wrapped;
    this.version = mine.keyVersion;
    if (!seen || seen.version !== this.version) await this.o.saveKeySeen(this.cave, { version: this.version, keyId: opened.id });
    return true;
  }

  private waitForKey(): void {
    this.status('waiting-key');
    this.later(() => void this.open(), 30_000);
  }

  private async handKeysToMyOtherDevices(): Promise<void> {
    const { devices } = await this.call<{ devices: DeviceInfo[] }>('GET', `/api/caves/${this.cave}/missing`);
    if (!devices.length || !this.wrappedMine) return;
    // An extractable copy only for wrapping, dropped when this returns; the everyday key stays non-extractable.
    const { key } = await unwrapCaveKey(this.wrappedMine, this.o.device.privateKey, { cave: this.cave, version: this.version, myPublicKey: this.o.device.publicKey }, { extractable: true });
    await this.call('POST', `/api/caves/${this.cave}/keys`, { version: this.version, wrapped: await this.wrapForDevices(key, devices) });
  }

  private lockCtx(kind: 'entry' | 'snapshot', author: string) {
    return { cave: this.cave, version: this.version, author, kind } as const;
  }

  /** Uploads a locked whole-cave snapshot covering entries up to `upTo` (the number is inside the lock too). */
  private async putSnapshot(upTo: number): Promise<void> {
    const body = JSON.stringify({ upTo, ops: this.o.engine.snapshot() });
    const locked = await lock(this.key!, body, this.lockCtx('snapshot', this.o.device.id));
    await this.call('PUT', `/api/caves/${this.cave}/snapshot`, { upTo, version: this.version, locked });
  }

  private async catchUpFromSnapshot(): Promise<void> {
    const { snapshot } = await this.call<{ snapshot: { upTo: number; keyVersion: number; locked: string } | null }>('GET', `/api/caves/${this.cave}/snapshot`);
    // A fresh device (last 0) takes even the snapshot made with the room, which covers no entries yet.
    if (!snapshot || !(snapshot.upTo > this.last || this.last === 0)) return;
    if (snapshot.keyVersion !== this.version) throw new Refused('snapshot under another key version');
    // The snapshot names no author, so it may be from any of this member's devices.
    const { devices } = await this.call<{ devices: DeviceInfo[] }>('GET', '/api/devices');
    let body: { upTo?: unknown; ops?: unknown } | null = null;
    for (const d of devices) {
      try {
        body = JSON.parse(await unlock(this.key!, snapshot.locked, this.lockCtx('snapshot', d.id)));
        break;
      } catch {
        // not this device's
      }
    }
    if (!body || typeof body.ops !== 'string' || typeof body.upTo !== 'number') throw new Refused('snapshot does not unlock');
    if (body.upTo !== snapshot.upTo) throw new Refused('snapshot number does not match');
    if (body.upTo < this.last) return;
    this.o.engine.apply(body.ops);
    this.last = body.upTo;
    await this.o.saveLast(this.cave, this.last);
  }

  private async listen(): Promise<void> {
    while (!this.stopped) {
      try {
        await this.catchUpFromSnapshot();
        this.abort = new AbortController();
        const res = await this.o.fetch(`${this.o.base}/api/caves/${this.cave}/stream?after=${this.last}`, { headers: this.headers(), credentials: 'same-origin', signal: this.abort.signal });
        if (!res.ok || !res.body) throw new Error('stream');
        const reader = res.body.getReader();
        const decoder = new TextDecoder();
        let buf = '';
        for (;;) {
          const { value, done } = await reader.read();
          if (done) break;
          buf += decoder.decode(value, { stream: true });
          let i;
          while ((i = buf.indexOf('\n')) >= 0) {
            const line = JSON.parse(buf.slice(0, i)) as StreamLine;
            buf = buf.slice(i + 1);
            await this.onLine(line);
          }
        }
      } catch (e) {
        if (e instanceof Refused) return this.fail();
        if (this.stopped) return;
      }
      // The stream ended (or the server dropped a client that fell behind): reconnect with after=<last>.
      this.live = false;
      this.status('offline');
      await new Promise((r) => setTimeout(r, 3_000));
    }
  }

  private async onLine(line: StreamLine): Promise<void> {
    if (line.t === 'live') {
      this.live = true;
      this.status(this.pending.length ? 'offline' : 'synced');
      void this.flush();
      return;
    }
    if (line.t !== 'entry' || typeof line.number !== 'number' || line.number <= this.last) return;
    if (line.keyVersion !== this.version || typeof line.locked !== 'string' || typeof line.authorDevice !== 'string' || !line.authorDevice) throw new Refused('entry malformed');
    // Unlocked even when it is our own, so an entry relabelled as ours fails here rather than being skipped.
    let ops: string;
    try {
      ops = await unlock(this.key!, line.locked, this.lockCtx('entry', line.authorDevice));
    } catch {
      throw new Refused('entry does not unlock');
    }
    if (line.authorDevice !== this.o.device.id) this.o.engine.apply(ops);
    this.last = line.number;
    this.sinceSnapshot++;
    await this.o.saveLast(this.cave, this.last);
    if (this.sinceSnapshot >= (this.o.snapshotEvery ?? 500)) {
      this.sinceSnapshot = 0;
      void this.putSnapshot(this.last).catch(() => {});
    }
  }

  /** Locks and posts local edits; keeps them in order while offline. */
  async flush(): Promise<void> {
    if (!this.key || this.flushing || this.failed) return;
    this.flushing = true;
    try {
      const ops = this.o.engine.outbox();
      if (ops && ops !== '[]') this.pending.push(await lock(this.key, ops, this.lockCtx('entry', this.o.device.id)));
      while (this.pending.length && !this.stopped) {
        try {
          await this.call('POST', `/api/caves/${this.cave}/entries`, { version: this.version, locked: this.pending[0] });
          this.pending.shift();
        } catch {
          this.status('offline');
          return;
        }
      }
      if (this.live) this.status('synced');
    } finally {
      this.flushing = false;
    }
  }
}
