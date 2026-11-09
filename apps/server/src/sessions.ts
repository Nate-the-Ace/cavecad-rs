// CaveCAD's own sessions: a random token the browser keeps in a cookie (web) or the desktop app sends as a
// bearer token. Only the token's SHA-256 is stored. 30 days, as the offline pass.
import { createHash, randomBytes } from 'node:crypto';
import type { Db } from './db.ts';
import type { Identity } from './members.ts';

export const SESSION_MS = 30 * 86_400_000;
/** The cookie's name: __Host- (pinned to this exact host, Secure, Path=/) when served over https; plain on http://localhost dev. */
export const sessionCookieName = (secure: boolean) => (secure ? '__Host-cavecad_session' : 'cavecad_session');
const hash = (t: string) => createHash('sha256').update(t).digest('hex');
const norm = (e: string) => e.trim().toLowerCase();

/** Starts a session for `who`; gives back the token (never stored). */
export function createSession(db: Db, who: Identity, kind: 'web' | 'desktop', now: number): string {
  db.prepare('DELETE FROM sessions WHERE expires_at < ?').run(now - SESSION_MS);
  const token = randomBytes(32).toString('base64url');
  db.prepare('INSERT INTO sessions (token_hash, email, name, provider, kind, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?)')
    .run(hash(token), norm(who.email), who.name.slice(0, 80), who.provider, kind, now, now + SESSION_MS);
  return token;
}

/** Who a live session belongs to, or null. With `kind`, only a session of that kind counts. */
export function findSession(db: Db, token: string, now: number, kind?: 'web' | 'desktop'): Identity | null {
  if (!token) return null;
  const r = db.prepare('SELECT email, name, provider FROM sessions WHERE token_hash = ? AND revoked_at IS NULL AND expires_at > ? AND (? IS NULL OR kind = ?)').get(hash(token), now, kind ?? null, kind ?? null) as
    | { email: string; name: string; provider: Identity['provider'] }
    | undefined;
  return r ? { email: r.email, name: r.name || r.email.split('@')[0], provider: r.provider, subject: 'session' } : null;
}

export function revokeSession(db: Db, token: string, now: number): void {
  if (token) db.prepare('UPDATE sessions SET revoked_at = ? WHERE token_hash = ? AND revoked_at IS NULL').run(now, hash(token));
}

/** Ends every session of this email (a removed member is signed out everywhere). */
export function revokeAllFor(db: Db, email: string, now: number): void {
  db.prepare('UPDATE sessions SET revoked_at = ? WHERE email = ? AND revoked_at IS NULL').run(now, norm(email));
}

export const sessionCookie = (token: string, secure: boolean) =>
  `${sessionCookieName(secure)}=${token}; HttpOnly;${secure ? ' Secure;' : ''} SameSite=Lax; Path=/; Max-Age=${SESSION_MS / 1000}`;
export const clearSessionCookie = (secure: boolean) => `${sessionCookieName(secure)}=; HttpOnly;${secure ? ' Secure;' : ''} SameSite=Lax; Path=/; Max-Age=0`;

/** One cookie's value from a Cookie header; '' when absent. */
export function readCookie(header: string | undefined, name: string): string {
  for (const part of (header ?? '').split(';')) {
    const i = part.indexOf('=');
    if (i > 0 && part.slice(0, i).trim() === name) return part.slice(i + 1).trim();
  }
  return '';
}
