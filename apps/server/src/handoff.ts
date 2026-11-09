// The desktop app's sign-in hand-off: after someone signs in in the browser, the server gives the browser a
// one-time code for cavecad://auth. Only the app that made the PKCE challenge can redeem it (its verifier), once,
// within a minute; it gets a desktop session back.
import { createHash, randomBytes } from 'node:crypto';
import type { Db } from './db.ts';
import type { Identity } from './members.ts';

const hash = (t: string) => createHash('sha256').update(t).digest('hex');

export function createHandoff(db: Db, who: Identity, challenge: string, now: number): string {
  const code = randomBytes(24).toString('base64url');
  db.prepare('DELETE FROM handoffs WHERE expires_at < ?').run(now - 3_600_000);
  db.prepare('INSERT INTO handoffs (code_hash, challenge, email, name, provider, expires_at) VALUES (?, ?, ?, ?, ?, ?)').run(hash(code), challenge, who.email, who.name, who.provider, now + 60_000);
  return code;
}

export function redeemHandoff(db: Db, code: string, verifier: string, now: number): Identity | null {
  const row = db.prepare('SELECT challenge, email, name, provider, expires_at, used_at FROM handoffs WHERE code_hash = ?').get(hash(String(code))) as
    | { challenge: string; email: string; name: string; provider: Identity['provider']; expires_at: number; used_at: number | null }
    | undefined;
  if (!row || row.used_at !== null || row.expires_at <= now) return null;
  if (createHash('sha256').update(String(verifier)).digest('base64url') !== row.challenge) return null;
  db.prepare('UPDATE handoffs SET used_at = ? WHERE code_hash = ?').run(now, hash(String(code)));
  return { email: row.email, name: row.name, provider: row.provider, subject: 'session' };
}
