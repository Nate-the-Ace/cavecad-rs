// The identity adapter: the one place that decides who a request is. Since 2026-11 CaveCAD signs people in
// itself (signin.ts): a browser carries a session cookie, the desktop app a bearer session token. A private
// copy reached through `tailscale serve`, and local development, have their own identities.
import type { Db } from './db.ts';
import type { Identity } from './members.ts';
import { findSession, readCookie, SESSION_COOKIE } from './sessions.ts';

export type RequestLike = { socket: { remoteAddress?: string }; headers: Record<string, string | string[] | undefined> };
export type WhoIsOptions = {
  db: Db;
  /** Reached through `tailscale serve` on this machine: its Tailscale-User-* headers are believed from 127.0.0.1 alone. */
  tailnet?: boolean;
  /** For local development only: everyone on this machine is this email. */
  devIdentity?: string;
  now?: () => number;
};

const plain = (addr = '') => addr.replace(/^::ffff:/, '');
const header = (r: RequestLike, name: string) => {
  const v = r.headers[name];
  return (Array.isArray(v) ? v[0] : v)?.trim() ?? '';
};
/** A header value tailscale serve wrote: plain ASCII, or RFC 2047 Q-encoded UTF-8 (`=?utf-8?q?Jos=C3=A9?=`). */
function qDecoded(v: string): string {
  const m = /^=\?utf-8\?q\?(.*)\?=$/i.exec(v);
  if (!m) return v;
  const bytes = m[1].replace(/_/g, ' ').replace(/=([0-9A-Fa-f]{2})/g, (_, h: string) => String.fromCharCode(parseInt(h, 16)));
  return Buffer.from(bytes, 'latin1').toString('utf8');
}
const local = (addr: string) => addr === '127.0.0.1' || addr === '::1';

/** Builds `whoIs(request) -> Identity | null` for these settings. */
export function makeWhoIs(o: WhoIsOptions): (r: RequestLike) => Promise<Identity | null> {
  const now = o.now ?? Date.now;
  return async (r) => {
    const addr = plain(r.socket.remoteAddress);
    const bearer = /^Bearer (\S+)$/.exec(header(r, 'authorization'))?.[1];
    if (bearer) return findSession(o.db, bearer, now());
    const cookie = readCookie(header(r, 'cookie'), SESSION_COOKIE);
    if (cookie) {
      const found = findSession(o.db, cookie, now());
      if (found) return found;
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
