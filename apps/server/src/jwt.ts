// Checks a signed JWT (RS256 or EdDSA) against a set of public keys, and fetches a key set from a JWKS URL.
// Google's ID tokens are checked here.
import { createPublicKey, verify, type JsonWebKey } from 'node:crypto';

const LEEWAY = 60;
const isObject = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);

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

export type JwtCheck = { keys: (kid?: string) => Promise<JsonWebKey[]>; issuers: string[]; audience: string; now: number };

/** The token's claims when its signature, issuer, audience and times check out; else null. */
export async function verifyJwt(token: string, o: JwtCheck): Promise<Record<string, unknown> | null> {
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
  const candidates = (await o.keys(kid)).filter((k) => keyFits(k, alg) && (kid === undefined || (k as { kid?: string }).kid === kid));
  const data = Buffer.from(parts[0] + '.' + parts[1]);
  const sig = Buffer.from(parts[2], 'base64url');
  if (!candidates.some((k) => checkSignature(k, alg, data, sig))) return null;
  const aud = Array.isArray(claims.aud) ? claims.aud : [claims.aud];
  if (typeof claims.iss !== 'string' || !o.issuers.includes(claims.iss) || !aud.includes(o.audience)) return null;
  if (aud.length > 1 && claims.azp !== o.audience) return null;
  if (typeof claims.exp !== 'number' || claims.exp + LEEWAY <= o.now) return null;
  if (claims.nbf !== undefined && (typeof claims.nbf !== 'number' || claims.nbf > o.now + LEEWAY)) return null;
  return claims;
}

/**
 * Fetches a JWKS. Keys are cached for an hour, a failed fetch for 30 s (no keys meanwhile), and a token
 * whose kid is unknown triggers at most one refetch a minute.
 */
export function jwksKeys(url: string, fetchImpl: typeof fetch = fetch): (kid?: string) => Promise<JsonWebKey[]> {
  let cache: { at: number; keys: JsonWebKey[] } | null = null;
  let failedAt = -Infinity;
  let refetchedAt = -Infinity;
  const load = async (): Promise<JsonWebKey[]> => {
    try {
      const res = await fetchImpl(url, { signal: AbortSignal.timeout(5000) });
      if (!res.ok) throw new Error(`${url} answered ${res.status}`);
      const set = (await res.json()) as { keys?: unknown };
      if (!Array.isArray(set?.keys)) throw new Error('no keys');
      cache = { at: Date.now(), keys: set.keys as JsonWebKey[] };
      return cache.keys;
    } catch (e) {
      failedAt = Date.now();
      console.error('JWKS:', e instanceof Error ? e.message : e);
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
