// Live listeners: for each cave, the open streams waiting for new entries. In memory only; a restart drops them
// and devices reconnect, asking for what they missed by number.
import type { ServerResponse } from 'node:http';

/** What a stream writes to: a ServerResponse, or a stand-in in tests. */
export type Out = Pick<ServerResponse, 'write' | 'destroy' | 'writableLength' | 'destroyed'>;
/** One line of a stream; entries carry their number. */
export type Line = { t: string; number?: number } & Record<string, unknown>;

/** A client with more unsent bytes than this is not keeping up: it is dropped, and reconnects with after=<last>. */
export const MAX_BUFFERED = 8 * 1024 * 1024;
/**
 * Streams one member may hold open at once (all caves, all devices). Opening one more closes their oldest, so a
 * client that reconnects in a loop without closing can't pile up sockets and buffers on the server.
 */
export const MAX_PER_MEMBER = 20;

/** Writes one line; drops the client if its unsent bytes pass MAX_BUFFERED. */
export function writeOrDrop(res: Out, text: string): void {
  if (res.destroyed) return;
  res.write(text);
  if (res.writableLength > MAX_BUFFERED) res.destroy();
}

/**
 * One open stream. Until `goLive`, new lines are queued (the stream is still catching up from the database);
 * `goLive(last)` writes the `live` line, then the queued lines numbered after `last`, then passes lines straight on.
 */
export class Listener {
  private queue: { number: number; text: string }[] | null = [];
  private last = 0;
  private stopped = false;
  readonly res: Out;
  private readonly unlink: () => void;
  constructor(res: Out, unlink: () => void) {
    this.res = res;
    this.unlink = unlink;
  }

  /** Takes one broadcast line. */
  deliver(line: Line, text: string): void {
    if (this.stopped) return;
    const number = line.number ?? 0;
    if (this.queue) return void this.queue.push({ number, text });
    if (number <= this.last) return;
    this.last = number;
    writeOrDrop(this.res, text);
  }

  /** Catch-up has written every entry up to `last`: goes live. */
  goLive(last: number): void {
    if (this.stopped || !this.queue) return;
    const queued = this.queue;
    this.queue = null;
    this.last = last;
    writeOrDrop(this.res, JSON.stringify({ t: 'live' }) + '\n');
    for (const q of queued) {
      if (q.number <= this.last) continue;
      this.last = q.number;
      writeOrDrop(this.res, q.text);
    }
  }

  /** Stops listening; safe to call more than once. */
  stop(): void {
    if (this.stopped) return;
    this.stopped = true;
    this.queue = null;
    this.unlink();
  }
}

const byCave = new Map<string, Set<Listener>>();
const byMember = new Map<string, Listener[]>();

/** Starts listening for a cave's new entries on an open stream (queued until `goLive`). */
export function listen(caveId: string, memberId: string, res: Out): Listener {
  let set = byCave.get(caveId);
  if (!set) byCave.set(caveId, (set = new Set()));
  const mine = byMember.get(memberId) ?? [];
  byMember.set(memberId, mine);
  const cave = set;
  const listener: Listener = new Listener(res, () => {
    cave.delete(listener);
    if (cave.size === 0 && byCave.get(caveId) === cave) byCave.delete(caveId);
    const i = mine.indexOf(listener);
    if (i >= 0) mine.splice(i, 1);
    if (mine.length === 0 && byMember.get(memberId) === mine) byMember.delete(memberId);
  });
  cave.add(listener);
  mine.push(listener);
  while (mine.length > MAX_PER_MEMBER) {
    const oldest = mine[0];
    oldest.stop();
    oldest.res.destroy();
  }
  return listener;
}

/** Sends one JSON line to every open stream of a cave. */
export function broadcast(caveId: string, line: Line): void {
  const text = JSON.stringify(line) + '\n';
  for (const l of [...(byCave.get(caveId) ?? [])]) l.deliver(line, text);
}

/** How many streams are open for a cave (for tests). */
export function listeners(caveId: string): number {
  return byCave.get(caveId)?.size ?? 0;
}
