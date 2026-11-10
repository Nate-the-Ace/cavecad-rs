// The offline pass: a signed note saying who a member is, good for 30 days, so a device that has
// signed in keeps working underground. Format: base64url(JSON claims) + "." + base64url(Ed25519 signature).
import { createPublicKey, sign, type KeyObject } from 'node:crypto';

export const PASS_DAYS = 30;
export type PassClaims = { v: 1; memberId: string; email: string; role: 'admin' | 'member'; issued: number; expires: number };

/** Signs a pass for this member, issued at `now`. */
export function signPass(key: KeyObject, who: { memberId: string; email: string; role: 'admin' | 'member' }, now: number): string {
  const claims: PassClaims = { v: 1, memberId: who.memberId, email: who.email, role: who.role, issued: now, expires: now + PASS_DAYS * 86_400_000 };
  const body = Buffer.from(JSON.stringify(claims)).toString('base64url');
  return body + '.' + sign(null, Buffer.from(body), key).toString('base64url');
}

/** The public half as the JWK `x` value the web build carries (`VITE_PASS_PUBLIC_KEY`). */
export function publicX(key: KeyObject): string {
  return (createPublicKey(key).export({ format: 'jwk' }) as { x: string }).x;
}
