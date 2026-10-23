// Reads and checks the offline pass the server gave this device (see apps/server/src/pass.ts), so the app
// can open without a network. The public key comes from the build (VITE_PASS_PUBLIC_KEY).
export type PassClaims = { v: 1; memberId: string; email: string; role: 'admin' | 'member'; issued: number; expires: number };
export type PassCheck = { ok: true; claims: PassClaims } | { ok: false; why: 'malformed' | 'signature' | 'expired' | 'no-key' };

const fromB64url = (s: string) => Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), (c) => c.charCodeAt(0));

/** Checks a pass: well formed, signed by the key whose JWK `x` is `publicX`, and not expired at `now`. */
export async function verifyPass(pass: string, publicX: string, now: number): Promise<PassCheck> {
  if (!publicX) return { ok: false, why: 'no-key' };
  const parts = pass.split('.');
  if (parts.length !== 2) return { ok: false, why: 'malformed' };
  let claims: PassClaims;
  try {
    claims = JSON.parse(new TextDecoder().decode(fromB64url(parts[0])));
  } catch {
    return { ok: false, why: 'malformed' };
  }
  if (claims?.v !== 1 || typeof claims.expires !== 'number') return { ok: false, why: 'malformed' };
  let good = false;
  try {
    const key = await crypto.subtle.importKey('jwk', { kty: 'OKP', crv: 'Ed25519', x: publicX }, { name: 'Ed25519' }, false, ['verify']);
    good = await crypto.subtle.verify({ name: 'Ed25519' }, key, fromB64url(parts[1]), new TextEncoder().encode(parts[0]));
  } catch {
    good = false;
  }
  if (!good) return { ok: false, why: 'signature' };
  if (claims.expires <= now) return { ok: false, why: 'expired' };
  return { ok: true, claims };
}
