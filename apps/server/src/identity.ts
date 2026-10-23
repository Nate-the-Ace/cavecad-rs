// The identity adapter: the one place that decides who a request is. Today a person signs in through
// TinyAuth, which Caddy asks before passing the request on and which adds Remote-* headers; the desktop
// app sends a TinyAuth OIDC ID token instead. At public launch this file is replaced by CaveCAD's own
// sign-in and nothing else in the server changes.
import { createPublicKey, verify, timingSafeEqual, type JsonWebKey } from 'node:crypto';
import type { Identity } from './members.ts';

export type RequestLike = { socket: { remoteAddress?: string }; headers: Record<string, string | string[] | undefined> };
export type WhoIsOptions = {
  /** The edge server's address; Remote-* headers are believed from it alone. */
  edgeAddress: string;
  /** The secret Caddy sets as X-Cave-Gate; Remote-* headers are believed only alongside it. */
  gateSecret: string;
  /** TinyAuth's OIDC provider, for bearer tokens from the desktop app. `keys(kid)` may refetch for an unknown kid. */
  oidc?: { issuer: string; audience: string; keys: (kid?: string) => Promise<JsonWebKey[]> };
  /** Reached through `tailscale serve` on this machine: its Tailscale-User-* headers are believed from 127.0.0.1 alone
   *  (serve deletes any a visitor sends). Until CaveCAD has a public address. */
  tailnet?: boolean;
  /** For local development only: everyone on this machine is this email. */
  devIdentity?: string;
  nowSeconds?: () => number;
};

const LEEWAY = 60;
const plain = (addr = '') => addr.replace(/^::ffff:/, '');
const header = (r: RequestLike, name: string) => {
  const v = r.headers[name];
  return (Array.isArray(v) ? v[0] : v)?.trim() ?? '';
};
/** A header TinyAuth wrote as UTF-8: Node hands its bytes over as latin1. Bytes that are not UTF-8 stay as they came. */
const utf8Header = (r: RequestLike, name: string) => {
  const v = header(r, name);
  const decoded = Buffer.from(v, 'latin1').toString('utf8');
  return decoded.includes('�') ? v : decoded;
};
/** A header value tailscale serve wrote: plain ASCII, or RFC 2047 Q-encoded UTF-8 (`=?utf-8?q?Jos=C3=A9?=`). */
function qDecoded(v: string): string {
  const m = /^=\?utf-8\?q\?(.*)\?=$/i.exec(v);
  if (!m) return v;
  const bytes = m[1].replace(/_/g, ' ').replace(/=([0-9A-Fa-f]{2})/g, (_, h: string) => String.fromCharCode(parseInt(h, 16)));
  return Buffer.from(bytes, 'latin1').toString('utf8');
}
const local = (addr: string) => addr === '127.0.0.1' || addr === '::1';
const isObject = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);

/** Compares two strings in constant time; different lengths are a mismatch. */
function sameSecret(given: string, secret: string): boolean {
  const a = Buffer.from(given);
  const b = Buffer.from(secret);
  if (a.length !== b.length) return false;
  return timingSafeEqual(a, b);
}

/** Builds `whoIs(request) -> Identity | null` for these settings. */
export function makeWhoIs(o: WhoIsOptions): (r: RequestLike) => Promise<Identity | null> {
  const now = o.nowSeconds ?? (() => Math.floor(Date.now() / 1000));
  return async (r) => {
    const addr = plain(r.socket.remoteAddress);
    const bearer = /^Bearer (.+)$/.exec(header(r, 'authorization'))?.[1];
    if (bearer && o.oidc) return fromIdToken(bearer, o.oidc, now());
    if (o.edgeAddress && o.gateSecret && addr === o.edgeAddress && sameSecret(header(r, 'x-cave-gate'), o.gateSecret)) {
      const email = utf8Header(r, 'remote-email').toLowerCase();
      if (!email) return null;
      return { email, name: utf8Header(r, 'remote-name') || email.split('@')[0], provider: null, subject: utf8Header(r, 'remote-sub') };
    }
    if (o.tailnet && local(addr)) {
      const email = header(r, 'tailscale-user-login').toLowerCase();
      if (!email) return null;
      return { email, name: qDecoded(header(r, 'tailscale-user-name')) || email.split('@')[0], provider: null, subject: 'tailnet:' + email };
    }
    if (o.devIdentity && local(addr)) return { email: o.devIdentity.toLowerCase(), name: o.devIdentity.split('@')[0], provider: null, subject: 'dev' };
    return null;
  };
}

/** Whether this key may verify a token signed with `alg`. */
function keyFits(k: JsonWebKey, alg: string): boolean {
  if (k.use === 'enc') return false;
  if (alg === 'RS256') return k.kty === 'RSA';
  if (alg === 'EdDSA') return k.kty === 'OKP' && k.crv === 'Ed25519';
  return false;
}

function checkSignature(jwk: JsonWebKey, alg: string, data: Buffer, sig: Buffer): boolean {
  try {
    const key = createPublicKey({ key: jwk, format: 'jwk' });
    if (alg === 'RS256') return verify('sha256', data, key, sig);
    if (alg === 'EdDSA') return verify(null, data, key, sig);
  } catch {
    // an unusable key verifies nothing
  }
  return false;
}

async function fromIdToken(token: string, oidc: NonNullable<WhoIsOptions['oidc']>, now: number): Promise<Identity | null> {
  const parts = token.split('.');
  if (parts.length !== 3) return null;
  let head: Record<string, unknown>, claims: Record<string, unknown>;
  try {
    const h: unknown = JSON.parse(Buffer.from(parts[0], 'base64url').toString());
    const c: unknown = JSON.parse(Buffer.from(parts[1], 'base64url').toString());
    if (!isObject(h) || !isObject(c)) return null;
    head = h;
    claims = c;
  } catch {
    return null;
  }
  const alg = typeof head.alg === 'string' ? head.alg : '';
  if (alg !== 'RS256' && alg !== 'EdDSA') return null;
  const kid = typeof head.kid === 'string' ? head.kid : undefined;
  const candidates = (await oidc.keys(kid)).filter((k) => keyFits(k, alg) && (kid === undefined || (k as { kid?: string }).kid === kid));
  if (!candidates.length) return null;
  const data = Buffer.from(parts[0] + '.' + parts[1]);
  const sig = Buffer.from(parts[2], 'base64url');
  if (!candidates.some((k) => checkSignature(k, alg, data, sig))) return null;

  const aud = Array.isArray(claims.aud) ? claims.aud : [claims.aud];
  if (claims.iss !== oidc.issuer || !aud.includes(oidc.audience)) return null;
  if (aud.length > 1 && claims.azp !== oidc.audience) return null;
  if (typeof claims.exp !== 'number' || claims.exp + LEEWAY <= now) return null;
  if (claims.nbf !== undefined && (typeof claims.nbf !== 'number' || claims.nbf > now + LEEWAY)) return null;
  if ('email_verified' in claims && claims.email_verified !== true) return null;
  if (typeof claims.email !== 'string' || !claims.email) return null;
  const email = claims.email.toLowerCase();
  return { email, name: typeof claims.name === 'string' ? claims.name : email.split('@')[0], provider: null, subject: String(claims.sub ?? '') };
}

/**
 * Fetches TinyAuth's signing keys from its discovery document. Keys are cached for an hour, a failed
 * fetch for 30 s (no keys meanwhile), and a token whose kid is unknown triggers at most one refetch a minute.
 */
export function discoveredKeys(issuer: string, fetchImpl: typeof fetch = fetch): (kid?: string) => Promise<JsonWebKey[]> {
  let cache: { at: number; keys: JsonWebKey[] } | null = null;
  let failedAt = -Infinity;
  let refetchedAt = -Infinity;
  const getJson = async (url: string) => {
    const res = await fetchImpl(url, { signal: AbortSignal.timeout(5000) });
    if (!res.ok) throw new Error(`${url} answered ${res.status}`);
    return (await res.json()) as unknown;
  };
  const load = async (): Promise<JsonWebKey[]> => {
    try {
      const conf = (await getJson(issuer.replace(/\/$/, '') + '/.well-known/openid-configuration')) as { jwks_uri?: unknown };
      if (typeof conf?.jwks_uri !== 'string') throw new Error('no jwks_uri');
      const set = (await getJson(conf.jwks_uri)) as { keys?: unknown };
      if (!Array.isArray(set?.keys)) throw new Error('no keys');
      cache = { at: Date.now(), keys: set.keys as JsonWebKey[] };
      return cache.keys;
    } catch (e) {
      failedAt = Date.now();
      console.error('OIDC keys:', e instanceof Error ? e.message : e);
      return cache && Date.now() - cache.at < 3_600_000 ? cache.keys : [];
    }
  };
  return async (kid) => {
    const t = Date.now();
    if (cache && t - cache.at < 3_600_000) {
      const known = kid === undefined || cache.keys.some((k) => (k as { kid?: string }).kid === kid);
      if (known || t - refetchedAt < 60_000 || t - failedAt < 30_000) return cache.keys;
      refetchedAt = t;
      return load();
    }
    if (t - failedAt < 30_000) return [];
    return load();
  };
}
