// The short text two people swap to connect, with no server in between. A code is a connection
// description (what WebRTC calls an SDP: how to reach this device, plus a fingerprint that lets the other
// side check it is talking to the right one) squeezed small enough to paste into a message. The host
// makes an INVITE code; the guest pastes it and gets back an ANSWER code; the host pastes that.
//
//   CAVE1-<text>        compressed (the usual case)
//   CAVE1u-<text>       not compressed, for a browser that cannot compress
//
// Codes are secrets for one use: whoever holds an invite code can answer it. Nothing in a code is
// the cave itself.

export type CodeKind = 'invite' | 'answer';

export interface Code {
  kind: CodeKind;
  /** The connection description. */
  sdp: string;
  /** Names the session, so an answer can be matched to the invite it replies to. */
  sid: string;
}

const PREFIX = 'CAVE1-';
const PLAIN = 'CAVE1u-';

const toBase64Url = (bytes: Uint8Array): string => {
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
};

const fromBase64Url = (text: string): Uint8Array => {
  const s = atob(text.replace(/-/g, '+').replace(/_/g, '/') + '==='.slice((text.length + 3) % 4));
  return Uint8Array.from(s, (c) => c.charCodeAt(0));
};

async function pipe(bytes: Uint8Array, stream: CompressionStream | DecompressionStream): Promise<Uint8Array> {
  const writer = stream.writable.getWriter();
  const fed = (async () => {
    await writer.write(bytes as unknown as BufferSource);
    await writer.close();
  })();
  const [, out] = await Promise.all([fed, new Response(stream.readable).arrayBuffer()]); // a bad input rejects here, to the caller
  return new Uint8Array(out);
}

const canCompress = (): boolean => typeof CompressionStream !== 'undefined' && typeof DecompressionStream !== 'undefined';

/** Makes the text to send. */
export async function encodeCode(code: Code): Promise<string> {
  const json = new TextEncoder().encode(JSON.stringify({ v: 1, k: code.kind, s: code.sdp, i: code.sid }));
  if (!canCompress()) return PLAIN + toBase64Url(json);
  return PREFIX + toBase64Url(await pipe(json, new CompressionStream('deflate-raw')));
}

/**
 * Reads pasted text back into a code. Forgiving about what surrounds it (spaces, line breaks, quotes, a
 * message wrapped around it); `null` for anything that is not a CaveCAD code.
 */
export async function decodeCode(text: string): Promise<Code | null> {
  const m = /CAVE1(u?)-([A-Za-z0-9_-]+)/.exec(text.replace(/\s+/g, ''));
  if (!m) return null;
  try {
    const bytes = fromBase64Url(m[2]);
    const json = m[1] === 'u' ? bytes : await pipe(bytes, new DecompressionStream('deflate-raw'));
    const d = JSON.parse(new TextDecoder().decode(json)) as { v?: number; k?: string; s?: unknown; i?: unknown };
    if (d.v !== 1 || (d.k !== 'invite' && d.k !== 'answer') || typeof d.s !== 'string' || typeof d.i !== 'string' || !d.s.startsWith('v=0') || d.i.length < 6) return null;
    return { kind: d.k, sdp: d.s, sid: d.i };
  } catch {
    return null;
  }
}
