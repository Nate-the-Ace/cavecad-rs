// Emailed sign-in codes: 6 digits, 10 minutes, 5 tries, once. Sent only to an email that may get in, but
// every request gets the same answer, so the form neither says who is a member nor mails strangers.
// The table keeps HMAC(secret, email + code), never the code.
import { createHmac, randomInt, timingSafeEqual } from 'node:crypto';
import type { Db } from './db.ts';

export const CODE_MS = 10 * 60_000;
const TRIES = 5;
const DAY_MS = 86_400_000;
const DAILY_FAILURES = 30;
const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

export type CodeDeps = { db: Db; secret: string; send: (to: string, code: string) => Promise<boolean>; mayGetIn: (email: string) => boolean };
const mac = (secret: string, email: string, code: string) => createHmac('sha256', secret).update(email + '\n' + code).digest('hex');

export async function requestCode(d: CodeDeps, raw: unknown, ip: string, now: number): Promise<{ ok: true } | { ok: false; error: 'bad-email' }> {
  const email = String(raw ?? '').trim().toLowerCase();
  if (!EMAIL.test(email) || email.length > 254) return { ok: false, error: 'bad-email' };
  const db = d.db;
  db.prepare('DELETE FROM code_requests WHERE at < ?').run(now - 86_400_000);
  const count = (sql: string, ...a: (string | number)[]) => (db.prepare(sql).get(...a) as { n: number }).n;
  const lastMinute = count('SELECT COUNT(*) AS n FROM code_requests WHERE email = ? AND at > ?', email, now - 60_000);
  const lastHour = count('SELECT COUNT(*) AS n FROM code_requests WHERE email = ? AND at > ?', email, now - 3_600_000);
  const fromIp = count('SELECT COUNT(*) AS n FROM code_requests WHERE ip = ? AND at > ?', ip, now - 3_600_000);
  if (lastMinute >= 1 || lastHour >= 5 || fromIp >= 20) return { ok: true };
  db.prepare('INSERT INTO code_requests (email, ip, at) VALUES (?, ?, ?)').run(email, ip, now);
  // A stranger gets a row too, holding the HMAC of a code nobody ever sees, so verify cannot tell them apart.
  const code = String(randomInt(0, 1_000_000)).padStart(6, '0');
  db.prepare('INSERT INTO email_codes (email, code_hmac, expires_at, tries_left, used_at) VALUES (?, ?, ?, ?, NULL) ON CONFLICT(email) DO UPDATE SET code_hmac = excluded.code_hmac, expires_at = excluded.expires_at, tries_left = excluded.tries_left, used_at = NULL')
    .run(email, mac(d.secret, email, code), now + CODE_MS, TRIES);
  if (!d.mayGetIn(email)) return { ok: true };
  // Never awaited, so neither timing nor a mailer error tells a member from a stranger.
  let sending: Promise<boolean>;
  try {
    sending = Promise.resolve(d.send(email, code));
  } catch (e) {
    sending = Promise.reject(e);
  }
  void sending.then(
    (ok) => { if (!ok) console.error('sign-in code: email to', email, 'failed'); },
    (e) => console.error('sign-in code: email failed', e),
  );
  return { ok: true };
}

export function verifyCode(d: CodeDeps, raw: unknown, rawCode: unknown, now: number): { ok: true; email: string } | { ok: false; error: 'expired' } | { ok: false; error: 'wrong'; triesLeft: number } {
  const email = String(raw ?? '').trim().toLowerCase();
  const code = String(rawCode ?? '').replace(/\s/g, '');
  d.db.prepare('DELETE FROM code_failures WHERE at < ?').run(now - DAY_MS);
  const failures = (d.db.prepare('SELECT COUNT(*) AS n FROM code_failures WHERE email = ? AND at > ?').get(email, now - DAY_MS) as { n: number }).n;
  if (failures >= DAILY_FAILURES) return { ok: false, error: 'expired' };
  const row = d.db.prepare('SELECT code_hmac, expires_at, tries_left, used_at FROM email_codes WHERE email = ?').get(email) as
    | { code_hmac: string; expires_at: number; tries_left: number; used_at: number | null }
    | undefined;
  if (!row || row.used_at !== null || row.expires_at <= now || row.tries_left <= 0) return { ok: false, error: 'expired' };
  const given = Buffer.from(mac(d.secret, email, code));
  if (/^\d{6}$/.test(code) && timingSafeEqual(given, Buffer.from(row.code_hmac))) {
    d.db.prepare('UPDATE email_codes SET used_at = ? WHERE email = ?').run(now, email);
    return { ok: true, email };
  }
  d.db.prepare('INSERT INTO code_failures (email, at) VALUES (?, ?)').run(email, now);
  const left = row.tries_left - 1;
  d.db.prepare('UPDATE email_codes SET tries_left = ? WHERE email = ?').run(left, email);
  return left > 0 ? { ok: false, error: 'wrong', triesLeft: left } : { ok: false, error: 'expired' };
}
