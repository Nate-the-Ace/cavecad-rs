// Sending a whole cave over a data channel: messages are cut into frames small enough for any browser
// (a channel refuses a big message outright) and put back together at the other end. Pure, so it is
// checked without a connection.

/** Biggest frame, in characters of text (well under every browser's limit). */
export const FRAME = 14_000;

interface Frame {
  /** The message this frame belongs to. */
  c: number;
  /** Which frame it is, counting from 0. */
  i: number;
  /** How many frames the message has. */
  n: number;
  /** The text. */
  d: string;
}

/** The frames of one message (always at least one). */
export function frames(message: string, id: number, size = FRAME): string[] {
  const n = Math.max(1, Math.ceil(message.length / size));
  return Array.from({ length: n }, (_, i) => JSON.stringify({ c: id, i, n, d: message.slice(i * size, (i + 1) * size) } satisfies Frame));
}

/** Puts frames back into messages, whatever order messages interleave in (frames of one message arrive in order). */
export class Reassembler {
  private parts = new Map<number, { n: number; got: string[] }>();

  /** Takes one frame; gives back the whole message when this was its last frame, else `null`. */
  push(text: string): string | null {
    let f: Frame;
    try {
      f = JSON.parse(text) as Frame;
    } catch {
      return null;
    }
    if (typeof f?.c !== 'number' || typeof f.i !== 'number' || typeof f.n !== 'number' || typeof f.d !== 'string' || f.n < 1 || f.n > 100_000 || f.i < 0 || f.i >= f.n) return null;
    let p = this.parts.get(f.c);
    if (!p) this.parts.set(f.c, (p = { n: f.n, got: [] }));
    if (f.i !== p.got.length || f.n !== p.n) {
      this.parts.delete(f.c); // out of order or inconsistent: drop the whole message rather than guess
      return null;
    }
    p.got.push(f.d);
    if (p.got.length < p.n) return null;
    this.parts.delete(f.c);
    return p.got.join('');
  }
}
