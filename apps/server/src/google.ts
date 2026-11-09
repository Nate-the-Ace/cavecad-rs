// Google sign-in done by the CaveCAD server: OAuth 2 authorization code with PKCE. The state, the PKCE
// verifier and where to go afterwards ride in a short-lived cookie signed with HMAC, so nothing is stored.
// The ID token is checked against Google's published keys, and the email must be one Google verified.
import { createHash, createHmac, randomBytes, timingSafeEqual, type JsonWebKey } from 'node:crypto';
import { verifyJwt, jwksKeys } from './jwt.ts';
import { readCookie } from './sessions.ts';
import type { Identity } from './members.ts';

const AUTH = 'https://accounts.google.com/o/oauth2/v2/auth';
const TOKEN = 'https://oauth2.googleapis.com/token';
const JWKS = 'https://www.googleapis.com/oauth2/v3/certs';
const ISSUERS = ['https://accounts.google.com', 'accounts.google.com'];
const COOKIE = 'cavecad_oauth';
const LIFE_MS = 600_000;

export type Desktop = { challenge: string; state: string } | null;
export type GoogleConfig = { clientId: string; clientSecret: string; redirectUri: string; stateSecret: string; fetch?: typeof fetch; keys?: (kid?: string) => Promise<JsonWebKey[]> };
type Kept = { state: string; verifier: string; returnTo: string; desktop: Desktop; exp: number };

const b64 = (b: Buffer) => b.toString('base64url');
const macOf = (secret: string, body: string) => b64(createHmac('sha256', secret).update(body).digest());

/** A path on this site to go back to; anything else becomes '/'. */
export function safeReturn(v: unknown): string {
  const s = typeof v === 'string' ? v : '';
  return s.startsWith('/') && !s.startsWith('//') && !s.startsWith('/\\') ? s : '/';
}

let defaultKeys: ((kid?: string) => Promise<JsonWebKey[]>) | null = null;

export function googleStart(c: GoogleConfig, p: { returnTo: string; desktop: Desktop }, now: number): { location: string; cookie: string } {
  const verifier = b64(randomBytes(32));
  const kept: Kept = { state: b64(randomBytes(16)), verifier, returnTo: safeReturn(p.returnTo), desktop: p.desktop, exp: now + LIFE_MS };
  const body = b64(Buffer.from(JSON.stringify(kept)));
  const u = new URL(AUTH);
  for (const [k, v] of Object.entries({ response_type: 'code', client_id: c.clientId, redirect_uri: c.redirectUri, scope: 'openid email profile', state: kept.state, code_challenge: b64(createHash('sha256').update(verifier).digest()), code_challenge_method: 'S256', prompt: 'select_account' })) u.searchParams.set(k, v);
  return { location: u.toString(), cookie: `${COOKIE}=${body}.${macOf(c.stateSecret, body)}; HttpOnly; Secure; SameSite=Lax; Path=/auth/google; Max-Age=${LIFE_MS / 1000}` };
}

function readKept(c: GoogleConfig, cookieHeader: string | undefined, now: number): Kept | null {
  const raw = readCookie(cookieHeader, COOKIE);
  const dot = raw.lastIndexOf('.');
  if (dot < 1) return null;
  const body = raw.slice(0, dot);
  const a = Buffer.from(raw.slice(dot + 1));
  const b = Buffer.from(macOf(c.stateSecret, body));
  if (a.length !== b.length || !timingSafeEqual(a, b)) return null;
  try {
    const k = JSON.parse(Buffer.from(body, 'base64url').toString()) as Kept;
    return k.exp > now ? k : null;
  } catch {
    return null;
  }
}

export const clearOauthCookie = `${COOKIE}=; HttpOnly; Secure; SameSite=Lax; Path=/auth/google; Max-Age=0`;

export async function googleFinish(c: GoogleConfig, q: URLSearchParams, cookieHeader: string | undefined, now: number): Promise<{ ok: true; who: Identity; returnTo: string; desktop: Desktop } | { ok: false; error: 'cancelled' | 'failed' | 'unverified' }> {
  const kept = readKept(c, cookieHeader, now);
  if (!kept || q.get('state') !== kept.state) return { ok: false, error: 'failed' };
  if (q.get('error')) return { ok: false, error: 'cancelled' };
  const code = q.get('code');
  if (!code) return { ok: false, error: 'failed' };
  try {
    const form = new URLSearchParams({ grant_type: 'authorization_code', code, redirect_uri: c.redirectUri, client_id: c.clientId, client_secret: c.clientSecret, code_verifier: kept.verifier });
    const res = await (c.fetch ?? fetch)(TOKEN, { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, body: form.toString(), signal: AbortSignal.timeout(10_000) });
    const out = (await res.json()) as { id_token?: string };
    if (!res.ok || !out.id_token) return { ok: false, error: 'failed' };
    const keys = c.keys ?? (defaultKeys ??= jwksKeys(JWKS));
    const claims = await verifyJwt(out.id_token, { keys, issuers: ISSUERS, audience: c.clientId, now: Math.floor(now / 1000) });
    if (!claims || typeof claims.email !== 'string') return { ok: false, error: 'failed' };
    if (claims.email_verified !== true) return { ok: false, error: 'unverified' };
    const email = claims.email.toLowerCase();
    return { ok: true, who: { email, name: typeof claims.name === 'string' ? claims.name : email.split('@')[0], provider: 'google', subject: String(claims.sub ?? '') }, returnTo: kept.returnTo, desktop: kept.desktop };
  } catch {
    return { ok: false, error: 'failed' };
  }
}
