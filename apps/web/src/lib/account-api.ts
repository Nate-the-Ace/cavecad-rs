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

export type ApiConfig = { base: string; fetch: typeof fetch; bearer?: () => string | null };

/** The account API at `base` ('' = this site). */
export function accountApi(c: ApiConfig) {
  const headers = (signup?: string) => {
    const h: Record<string, string> = { 'Content-Type': 'application/json' };
    const t = c.bearer?.();
    if (t) h.Authorization = 'Bearer ' + t;
    if (signup) h['X-Cave-Signup'] = signup;
    return h;
  };
  const call = async <T>(method: string, path: string, body?: unknown): Promise<T> => {
    const res = await c.fetch(c.base + path, { method, headers: headers(), body: body === undefined ? undefined : JSON.stringify(body), credentials: 'include', redirect: 'manual' });
    if (res.type === 'opaqueredirect' || res.status === 401) throw new AccountError('signed-out', 401);
    const out = (await res.json()) as T & { error?: string };
    if (!res.ok) throw new AccountError(out.error ?? 'error', res.status);
    return out;
  };

  return {
    /** Who is signed in; `signup` is the token from an invite link. Never throws. */
    async me(signup?: string): Promise<MeResult> {
      let res: Response;
      try {
        res = await c.fetch(c.base + '/api/me', { headers: headers(signup), credentials: 'include', redirect: 'manual' });
      } catch {
        return { kind: 'offline' };
      }
      if (res.type === 'opaqueredirect' || res.status === 401) return { kind: 'signed-out' };
      if (!res.ok) return { kind: 'offline' };
      const out = (await res.json()) as { status: string } & Record<string, unknown>;
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
