// Talks to the CaveCAD server's account API (apps/server/src/api.ts). Pure: the fetch, the address and the
// desktop app's bearer token are passed in, so node tests run it against the real server code.
export type Role = 'admin' | 'member';
export type DriveProvider = 'gdrive' | 'onedrive' | 'dropbox' | 'webdav';
export type Member = { id: string; email: string; displayName: string; colour: string; role: Role; provider: string | null; invitedBy: string | null; invitedByName?: string | null; createdAt: number; lastSeenAt: number | null; welcomedAt: number | null };
export type Drive = { id: string; provider: DriveProvider; accountLabel: string; createdAt: number };
export type Invite = { id: string; email: string; role: Role; invitedBy: string; createdAt: number; expiresAt: number };
export type NotInvitedReason = 'none' | 'expired' | 'used' | 'cancelled' | 'other-email';
export type MeResult =
  | { kind: 'member'; member: Member; drives: Drive[]; firstVisit: boolean; signups: 'invite' | 'open'; pass: string }
  | { kind: 'not-invited'; email: string; reason: NotInvitedReason; inviter?: string }
  | { kind: 'removed'; email: string }
  | { kind: 'signed-out' }
  | { kind: 'offline' };
export type InviteMade = { invite: Invite; link: string; emailed: boolean };

/** A refusal from the server, with its code (`not-admin`, `self`, `last-admin`, `already-member`, ...). */
export class AccountError extends Error {
  code: string;
  status: number;
  constructor(code: string, status: number) {
    super(code);
    this.code = code;
    this.status = status;
  }
}

export type ApiConfig = {
  base: string;
  fetch: typeof fetch;
  bearer?: () => string | null;
  /** The desktop app: on a 401, try to get a fresh token; true retries the request once. */
  onUnauthorized?: () => Promise<boolean>;
  /** A member-only call was refused as signed out, after any retry: the sign-in has expired. */
  onSignedOut?: () => void;
};

/** The account API at `base` ('' = this site). Same-origin credentials only: the web app is served by the same
 *  server, and the desktop app sends a bearer instead (the server never allows credentialed cross-origin calls). */
export function accountApi(c: ApiConfig) {
  const headers = (signup?: string) => {
    const h: Record<string, string> = { 'Content-Type': 'application/json' };
    const t = c.bearer?.();
    if (t) h.Authorization = 'Bearer ' + t;
    if (signup) h['X-Cave-Signup'] = signup;
    return h;
  };
  const refused = (res: Response) => res.type === 'opaqueredirect' || res.status === 401;
  /** Sends a request, and once more after a fresh desktop token when the first is refused as signed out. */
  const send = async (path: string, init: () => RequestInit): Promise<Response> => {
    const go = () => c.fetch(c.base + path, { ...init(), credentials: 'same-origin', redirect: 'manual' });
    const res = await go();
    if (refused(res) && c.onUnauthorized && (await c.onUnauthorized())) return go();
    return res;
  };
  const call = async <T>(method: string, path: string, body?: unknown): Promise<T> => {
    const res = await send(path, () => ({ method, headers: headers(), body: body === undefined ? undefined : JSON.stringify(body) }));
    if (refused(res)) {
      c.onSignedOut?.();
      throw new AccountError('signed-out', 401);
    }
    let out: T & { error?: string };
    try {
      out = (await res.json()) as T & { error?: string };
    } catch {
      throw new AccountError('error', res.status);
    }
    if (!res.ok) throw new AccountError(out?.error ?? 'error', res.status);
    return out;
  };

  return {
    /** Who is signed in; `signup` is the token from an invite link. Never throws. */
    async me(signup?: string): Promise<MeResult> {
      let res: Response;
      try {
        res = await send('/api/me', () => ({ headers: headers(signup) }));
      } catch {
        return { kind: 'offline' };
      }
      if (refused(res)) return { kind: 'signed-out' };
      if (!res.ok) return { kind: 'offline' };
      let out: { status: string } & Record<string, unknown>;
      try {
        out = (await res.json()) as { status: string } & Record<string, unknown>;
      } catch {
        return { kind: 'offline' }; // a page in the way (a captive portal, a proxy's error page) is not the server
      }
      if (!out || typeof out !== 'object') return { kind: 'offline' };
      if (out.status === 'member') return { kind: 'member', ...(out as unknown as Omit<Extract<MeResult, { kind: 'member' }>, 'kind'>) };
      if (out.status === 'removed') return { kind: 'removed', email: String(out.email) };
      return { kind: 'not-invited', email: String(out.email), reason: out.reason as NotInvitedReason, inviter: out.inviter as string | undefined };
    },
    updateMe: (p: { displayName?: string; colour?: string; welcomed?: boolean }) => call<{ member: Member }>('PATCH', '/api/me', p),
    addDrive: (provider: DriveProvider, accountLabel: string) => call<{ drive: Drive }>('POST', '/api/me/drives', { provider, accountLabel }),
    removeDrive: (id: string) => call<{ ok: true }>('DELETE', `/api/me/drives/${encodeURIComponent(id)}`),
    members: async () => (await call<{ members: Member[] }>('GET', '/api/admin/members')).members,
    setRole: (id: string, role: Role) => call<{ member: Member }>('PATCH', `/api/admin/members/${encodeURIComponent(id)}`, { role }),
    removeMember: (id: string) => call<{ ok: true }>('DELETE', `/api/admin/members/${encodeURIComponent(id)}`),
    invites: async () => (await call<{ invites: Invite[] }>('GET', '/api/admin/invites')).invites,
    invite: (email: string, role: Role) => call<InviteMade>('POST', '/api/admin/invites', { email, role }),
    resend: (id: string) => call<InviteMade>('POST', `/api/admin/invites/${encodeURIComponent(id)}/resend`),
    cancel: (id: string) => call<{ ok: true }>('DELETE', `/api/admin/invites/${encodeURIComponent(id)}`),
    setSignups: (signups: 'invite' | 'open') => call<{ signups: string }>('PUT', '/api/admin/settings/signups', { signups }),
  };
}
export type AccountApi = ReturnType<typeof accountApi>;

// ---- small rules the account page follows, kept here so node tests can check them ----

/** Where a kept offline pass leaves the app: still good, too old to edit with, or not to be trusted at all
 *  (no key in this build, unreadable, or not signed by CaveCAD), which signs out and forgets it. */
export function offlinePhase(why: 'ok' | 'expired' | 'no-key' | 'malformed' | 'signature'): 'offline' | 'offline-expired' | 'signed-out' {
  return why === 'ok' ? 'offline' : why === 'expired' ? 'offline-expired' : 'signed-out';
}

/** The member to show offline: who and what role come from the signed pass, only the name and colour
 *  from the copy kept beside it (which nothing vouches for). */
export function passMember(kept: Member, claims: { memberId: string; email: string; role: Role }): Member {
  return { ...kept, id: claims.memberId, email: claims.email, role: claims.role };
}

/** Wraps an async job so a call while one is running gets that same run instead of starting another. */
export function singleFlight<T>(job: () => Promise<T>): () => Promise<T> {
  let running: Promise<T> | null = null;
  return () => (running ??= job().finally(() => (running = null)));
}
