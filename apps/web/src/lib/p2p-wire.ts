// Sending a whole cave over a data channel: messages are cut into frames small enough for any browser
// (a channel refuses a big message outright) and put back together at the other end. Pure, so it is
// checked without a connection.

/** Biggest frame, in characters of text (well under every browser's limit). */
export const FRAME = 14_000;

/** The frames of one message (always at least one). A frame is `id.index.count.` and then its slice of the text: no
 *  escaping of the message inside another message, which would swell every quote mark. */
export function frames(message: string, id: number, size = FRAME): string[] {
  const n = Math.max(1, Math.ceil(message.length / size));
  return Array.from({ length: n }, (_, i) => `${id}.${i}.${n}.${message.slice(i * size, (i + 1) * size)}`);
}

interface Header {
  c: number;
  i: number;
  n: number;
  d: string;
}

/** Reads one frame: the compact form above, or the older JSON envelope `{c, i, n, d}` (a device on an earlier build). Null if it is neither. */
function readFrame(text: string): Header | null {
  if (text.charCodeAt(0) === 123 /* { */) {
    try {
      const f = JSON.parse(text) as Header;
      return typeof f?.c === 'number' && typeof f.i === 'number' && typeof f.n === 'number' && typeof f.d === 'string' ? f : null;
    } catch {
      return null;
    }
  }
  const a = text.indexOf('.');
  const b = text.indexOf('.', a + 1);
  const c = text.indexOf('.', b + 1);
  if (a < 1 || b < 0 || c < 0) return null;
  const f = { c: Number(text.slice(0, a)), i: Number(text.slice(a + 1, b)), n: Number(text.slice(b + 1, c)), d: text.slice(c + 1) };
  return Number.isInteger(f.c) && Number.isInteger(f.i) && Number.isInteger(f.n) ? f : null;
}

/** Puts frames back into messages, whatever order messages interleave in (frames of one message arrive in order). */
export class Reassembler {
  private parts = new Map<number, { n: number; got: string[] }>();

  /** Takes one frame; gives back the whole message when this was its last frame, else `null`. */
  push(text: string): string | null {
    const f = readFrame(text);
    if (!f || f.n < 1 || f.n > 100_000 || f.i < 0 || f.i >= f.n) return null;
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

/** Messages longer than this are squeezed before they are cut into frames (a short one is not worth the trouble). */
export const COMPRESS_OVER = 1500;
const MARK = '~';

function toBase64(bytes: Uint8Array): string {
  let out = '';
  for (let i = 0; i < bytes.length; i += 0x8000) out += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(out);
}

function fromBase64(text: string): Uint8Array {
  const raw = atob(text);
  return Uint8Array.from(raw, (c) => c.charCodeAt(0));
}

async function through(bytes: Uint8Array, stream: CompressionStream | DecompressionStream): Promise<Uint8Array> {
  return new Uint8Array(await new Response(new Blob([bytes as BlobPart]).stream().pipeThrough(stream)).arrayBuffer());
}

/** What goes into the frames for a message: the message itself, or (when long and it shrinks) a `~` and its deflated text in base64. */
export async function encodeMessage(text: string): Promise<string> {
  if (text.length <= COMPRESS_OVER || typeof CompressionStream === 'undefined') return text;
  try {
    const packed = MARK + toBase64(await through(new TextEncoder().encode(text), new CompressionStream('deflate-raw')));
    return packed.length < text.length ? packed : text;
  } catch {
    return text;
  }
}

/** The message in what came out of the frames (the inverse of `encodeMessage`). Throws if a squeezed message cannot be opened. */
export async function decodeMessage(payload: string): Promise<string> {
  if (payload.charCodeAt(0) !== MARK.charCodeAt(0)) return payload;
  return new TextDecoder().decode(await through(fromBase64(payload.slice(1)), new DecompressionStream('deflate-raw')));
}
