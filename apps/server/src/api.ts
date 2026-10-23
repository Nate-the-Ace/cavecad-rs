// The account API. Each request: who is this (identity adapter), are they a member (members.ts), then
// the route. Answers are JSON; rule errors from members.ts become 403/404/409 with their code.
import type { IncomingMessage, ServerResponse } from 'node:http';
import type { KeyObject } from 'node:crypto';
import type { Db } from './db.ts';
import { getSetting, setSetting } from './db.ts';
import * as M from './members.ts';
import { signPass } from './pass.ts';
import { sendInvite, type MailerConfig } from './mailer.ts';
import type { RequestLike } from './identity.ts';

export type ApiDeps = {
  db: Db;
  whoIs: (r: RequestLike) => Promise<M.Identity | null>;
  adminEmail: string;
  passKey: KeyObject;
  publicUrl: string;
  mailer: MailerConfig;
  /** Exchanges a desktop sign-in code with TinyAuth (Task 11); absent until configured. */
  desktopToken?: (body: Record<string, string>) => Promise<{ status: number; body: unknown }>;
  now?: () => number;
};

const DESKTOP_ORIGINS = ['tauri://localhost', 'http://tauri.localhost', 'https://tauri.localhost'];
const STATUS: Record<string, number> = { 'not-admin': 403, 'not-found': 404, self: 409, 'last-admin': 409, 'already-member': 409, 'bad-email': 400, 'bad-role': 400, 'bad-provider': 400, 'bad-name': 400, 'bad-colour': 400 };

function send(res: ServerResponse, status: number, body: unknown) {
  res.writeHead(status, { 'Content-Type': 'application/json', 'Cache-Control': 'no-store' });
  res.end(JSON.stringify(body));
}

class BadBody extends SyntaxError {}

/** Reads a JSON object body of at most 64 KB; anything else throws a SyntaxError (answered 400). */
async function readJson(req: IncomingMessage): Promise<Record<string, unknown>> {
  const chunks: Buffer[] = [];
  let size = 0;
  for await (const chunk of req) {
    const b = Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk);
    size += b.length;
    if (size > 64 * 1024) throw new BadBody('too-big');
    chunks.push(b);
  }
  const s = Buffer.concat(chunks).toString('utf8');
  if (!s) return {};
  const v: unknown = JSON.parse(s);
  if (typeof v !== 'object' || v === null || Array.isArray(v)) throw new BadBody('not-an-object');
  return v as Record<string, unknown>;
}

const SAFE = ['GET', 'HEAD', 'OPTIONS'];
const originOf = (u: string) => {
  try {
    return new URL(u).origin;
  } catch {
    return '';
  }
};

/** Refuses state-changing requests a browser could have sent from another site. Returns true when it answered. */
function refusedCrossSite(d: ApiDeps, req: IncomingMessage, res: ServerResponse, method: string, origin: string): boolean {
  if (SAFE.includes(method)) return false;
  const type = String(req.headers['content-type'] ?? '').toLowerCase();
  if (!type.startsWith('application/json')) return send(res, 415, { error: 'bad-request' }), true;
  const desktop = DESKTOP_ORIGINS.includes(origin);
  const bearer = /^Bearer \S/.test(String(req.headers.authorization ?? ''));
  const site = req.headers['sec-fetch-site'];
  if (site !== undefined && site !== 'same-origin' && site !== 'none' && !(desktop && bearer)) return send(res, 403, { error: 'cross-site' }), true;
  if (req.headers.origin !== undefined && origin !== originOf(d.publicUrl) && !desktop) return send(res, 403, { error: 'cross-site' }), true;
  return false;
}

/** Handles one `/api/...` request. */
export async function handleApi(d: ApiDeps, req: IncomingMessage, res: ServerResponse): Promise<void> {
  const now = (d.now ?? Date.now)();
  const origin = String(req.headers.origin ?? '');
  if (DESKTOP_ORIGINS.includes(origin)) {
    res.setHeader('Access-Control-Allow-Origin', origin);
    res.setHeader('Access-Control-Allow-Headers', 'Authorization, Content-Type, X-Cave-Signup');
    res.setHeader('Access-Control-Allow-Methods', 'GET, POST, PATCH, PUT, DELETE');
    res.setHeader('Vary', 'Origin');
  }
  if (req.method === 'OPTIONS') return void res.writeHead(204).end();
  const url = new URL(req.url ?? '/', 'http://x');
  const path = url.pathname;
  const method = req.method ?? 'GET';
  if (refusedCrossSite(d, req, res, method, origin)) return;

  try {
    if (path === '/api/desktop-token' && method === 'POST') {
      if (!d.desktopToken) return send(res, 404, { error: 'not-found' });
      const out = await d.desktopToken((await readJson(req)) as Record<string, string>);
      return send(res, out.status, out.body);
    }
  } catch (e) {
    if (e instanceof SyntaxError) return send(res, 400, { error: 'bad-request' });
    throw e;
  }

  const who = await d.whoIs(req as unknown as RequestLike);
  if (!who) return send(res, 401, { error: 'signed-out' });
  const token = String(req.headers['x-cave-signup'] ?? '') || undefined;
  const r = M.resolve(d.db, who, { adminEmail: d.adminEmail, now, token });
  if (r.status !== 'member') return path === '/api/me' && method === 'GET' ? send(res, 200, r) : send(res, 403, { error: r.status });
  const me = r.member;

  try {
    if (path === '/api/me' && method === 'GET') {
      return send(res, 200, { status: 'member', member: me, drives: M.listDrives(d.db, me.id), firstVisit: r.firstVisit, signups: getSetting(d.db, 'signups'), pass: signPass(d.passKey, { memberId: me.id, email: me.email, role: me.role }, now) });
    }
    if (path === '/api/me' && method === 'PATCH') {
      const b = await readJson(req);
      if (b.welcomed === true) M.markWelcomed(d.db, me.id, now);
      return send(res, 200, { member: M.updateProfile(d.db, me.id, { displayName: b.displayName as string | undefined, colour: b.colour as string | undefined }) });
    }
    if (path === '/api/me/drives' && method === 'POST') {
      const b = await readJson(req);
      return send(res, 200, { drive: M.addDrive(d.db, me.id, b.provider as M.DriveProvider, String(b.accountLabel ?? ''), now) });
    }
    const drive = /^\/api\/me\/drives\/([^/]+)$/.exec(path);
    if (drive && method === 'DELETE') {
      M.removeDrive(d.db, me.id, drive[1]);
      return send(res, 200, { ok: true });
    }

    if (path.startsWith('/api/admin/') && me.role !== 'admin') return send(res, 403, { error: 'not-admin' });
    if (path === '/api/admin/members' && method === 'GET') {
      const all = M.listMembers(d.db);
      const names = new Map(all.map((m) => [m.id, m.displayName]));
      return send(res, 200, { members: all.map((m) => ({ ...m, invitedByName: m.invitedBy ? names.get(m.invitedBy) ?? null : null })) });
    }
    const member = /^\/api\/admin\/members\/([^/]+)$/.exec(path);
    if (member && method === 'PATCH') return send(res, 200, { member: M.setRole(d.db, me.id, member[1], (await readJson(req)).role as M.Role) });
    if (member && method === 'DELETE') {
      M.removeMember(d.db, me.id, member[1], now);
      return send(res, 200, { ok: true });
    }
    if (path === '/api/admin/invites' && method === 'GET') return send(res, 200, { invites: M.listInvites(d.db, now) });
    const mail = async (made: { invite: M.Invite; token: string }) => {
      const link = `${d.publicUrl.replace(/\/$/, '')}/?signup=${made.token}`;
      const emailed = await sendInvite(d.mailer, { to: made.invite.email, inviter: me.displayName, link });
      return { invite: made.invite, link, emailed };
    };
    if (path === '/api/admin/invites' && method === 'POST') {
      const b = await readJson(req);
      return send(res, 200, await mail(M.createInvite(d.db, me.id, String(b.email ?? ''), b.role as M.Role, now)));
    }
    const invite = /^\/api\/admin\/invites\/([^/]+)(\/resend)?$/.exec(path);
    if (invite && invite[2] && method === 'POST') return send(res, 200, await mail(M.resendInvite(d.db, me.id, invite[1], now)));
    if (invite && !invite[2] && method === 'DELETE') {
      M.cancelInvite(d.db, me.id, invite[1], now);
      return send(res, 200, { ok: true });
    }
    if (path === '/api/admin/settings/signups' && method === 'PUT') {
      const v = (await readJson(req)).signups;
      if (v !== 'invite' && v !== 'open') return send(res, 400, { error: 'bad-setting' });
      setSetting(d.db, 'signups', v);
      return send(res, 200, { signups: v });
    }
    return send(res, 404, { error: 'not-found' });
  } catch (e) {
    const code = e instanceof Error ? e.message : 'error';
    if (STATUS[code]) return send(res, STATUS[code], { error: code });
    if (e instanceof SyntaxError) return send(res, 400, { error: 'bad-request' });
    throw e;
  }
}
