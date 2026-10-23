// The identity adapter: the one place that decides who a request is. Today a person signs in through
// TinyAuth, which Caddy asks before passing the request on and which adds Remote-* headers; the desktop
// app sends a TinyAuth OIDC ID token instead. At public launch this file is replaced by CaveCAD's own
// sign-in and nothing else in the server changes.
import { createPublicKey, verify, type JsonWebKey } from 'node:crypto';
import type { Identity } from './members.ts';

export type RequestLike = { socket: { remoteAddress?: string }; headers: Record<string, string | string[] | undefined> };
export type WhoIsOptions = {
  /** The edge server's address; Remote-* headers are believed from it alone. */
  edgeAddress: string;
  /** TinyAuth's OIDC provider, for bearer tokens from the desktop app. */
  oidc?: { issuer: string; audience: string; keys: () => Promise<JsonWebKey[]> };
  /** For local development only: everyone on this machine is this email. */
  devIdentity?: string;
  nowSeconds?: () => number;
};

const plain = (addr = '') => addr.replace(/^::ffff:/, '');
const header = (r: RequestLike, name: string) => {
  const v = r.headers[name];
  return (Array.isArray(v) ? v[0] : v)?.trim() ?? '';
};
const local = (addr: string) => addr === '127.0.0.1' || addr === '::1';

/** Builds `whoIs(request) -> Identity | null` for these settings. */
export function makeWhoIs(o: WhoIsOptions): (r: RequestLike) => Promise<Identity | null> {
  const now = o.nowSeconds ?? (() => Math.floor(Date.now() / 1000));
  return async (r) => {
    const addr = plain(r.socket.remoteAddress);
    const bearer = /^Bearer (.+)$/.exec(header(r, 'authorization'))?.[1];
    if (bearer && o.oidc) return fromIdToken(bearer, o.oidc, now());
    if (addr === o.edgeAddress && header(r, 'x-cave-gate') === 'tinyauth') {
      const email = header(r, 'remote-email').toLowerCase();
      if (!email) return null;
      return { email, name: header(r, 'remote-name') || email.split('@')[0], provider: null, subject: header(r, 'remote-sub') };
    }
    if (o.devIdentity && local(addr)) return { email: o.devIdentity.toLowerCase(), name: o.devIdentity.split('@')[0], provider: null, subject: 'dev' };
    return null;
  };
}

async function fromIdToken(token: string, oidc: NonNullable<WhoIsOptions['oidc']>, now: number): Promise<Identity | null> {
  const parts = token.split('.');
  if (parts.length !== 3) return null;
  let head: { alg?: string; kid?: string }, claims: Record<string, unknown>;
  try {
    head = JSON.parse(Buffer.from(parts[0], 'base64url').toString());
    claims = JSON.parse(Buffer.from(parts[1], 'base64url').toString());
  } catch {
    return null;
  }
  const jwk = (await oidc.keys()).find((k) => !head.kid || (k as { kid?: string }).kid === head.kid);
  if (!jwk) return null;
  const data = Buffer.from(parts[0] + '.' + parts[1]);
  const sig = Buffer.from(parts[2], 'base64url');
  let good = false;
  try {
    const key = createPublicKey({ key: jwk, format: 'jwk' });
    if (head.alg === 'RS256') good = verify('sha256', data, key, sig);
    else if (head.alg === 'EdDSA') good = verify(null, data, key, sig);
  } catch {
    good = false;
  }
  if (!good) return null;
  const aud = Array.isArray(claims.aud) ? claims.aud : [claims.aud];
  if (claims.iss !== oidc.issuer || !aud.includes(oidc.audience)) return null;
  if (typeof claims.exp !== 'number' || claims.exp <= now) return null;
  if (typeof claims.email !== 'string' || !claims.email) return null;
  const email = claims.email.toLowerCase();
  return { email, name: typeof claims.name === 'string' ? claims.name : email.split('@')[0], provider: null, subject: String(claims.sub ?? '') };
}

/** Fetches (and caches for an hour) TinyAuth's signing keys from its discovery document. */
export function discoveredKeys(issuer: string): () => Promise<JsonWebKey[]> {
  let cache: { at: number; keys: JsonWebKey[] } | null = null;
  return async () => {
    if (cache && Date.now() - cache.at < 3_600_000) return cache.keys;
    const conf = (await (await fetch(issuer.replace(/\/$/, '') + '/.well-known/openid-configuration')).json()) as { jwks_uri: string };
    const keys = ((await (await fetch(conf.jwks_uri)).json()) as { keys: JsonWebKey[] }).keys;
    cache = { at: Date.now(), keys };
    return keys;
  };
}
