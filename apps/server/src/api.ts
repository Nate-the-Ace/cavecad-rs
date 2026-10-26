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
import { serveStatic } from './static.ts';
import { send, sendBadBody, readJson } from './http.ts';
import { handleRooms } from './rooms-api.ts';
import { ownsAnyCave } from './rooms.ts';

export type ApiDeps = {
  db: Db;
  whoIs: (r: RequestLike) => Promise<M.Identity | null>;
  adminEmail: string;
  passKey: KeyObject;
  publicUrl: string;
  mailer: MailerConfig;
  /** TinyAuth's sign-out page, offered on the not-invited page to try another account; empty for none. */
  signoutUrl?: string;
  /** Reached through `tailscale serve` only: /api/me says so, and the page hides sign-out and switching account. */
  tailnet?: boolean;
  /** Exchanges a desktop sign-in code with TinyAuth (Task 11); absent until configured. */
  desktopToken?: (body: Record<string, string>) => Promise<{ status: number; body: unknown }>;
  /** Where the desktop app sends the browser to sign in (from TinyAuth's discovery document); absent until configured. */
  desktopConfig?: () => Promise<{ authorizationEndpoint: string; clientId: string } | null>;
  now?: () => number;
};

const DESKTOP_ORIGINS = ['tauri://localhost', 'http://tauri.localhost', 'https://tauri.localhost'];
const STATUS: Record<string, number> = { 'not-admin': 403, 'not-found': 404, self: 409, 'last-admin': 409, 'already-member': 409, 'bad-email': 400, 'bad-role': 400, 'bad-provider': 400, 'bad-name': 400, 'bad-colour': 400 };

const SAFE = ['GET', 'HEAD', 'OPTIONS'];
const originOf = (u: string) => {
  try {
    return new URL(u).origin;
  } catch {
    return '';
  }
};

/** Refuses state-changing requests a browser could have sent from another site. Returns true when it answered. */
function refusedCrossSite(d: ApiDeps, req: IncomingMessage, res: ServerResponse, method: string, origin: string, path: string): boolean {
  if (SAFE.includes(method)) return false;
  const type = String(req.headers['content-type'] ?? '').toLowerCase();
  if (!type.startsWith('application/json')) return send(res, 415, { error: 'bad-request' }), true;
  const desktop = DESKTOP_ORIGINS.includes(origin);
  const bearer = /^Bearer \S/.test(String(req.headers.authorization ?? ''));
  const site = req.headers['sec-fetch-site'];
  // The desktop app fetches its token from /api/desktop-token, so it has no bearer there yet.
  const desktopTrusted = desktop && (bearer || (path === '/api/desktop-token' && method === 'POST'));
  if (site !== undefined && site !== 'same-origin' && site !== 'none' && !desktopTrusted) return send(res, 403, { error: 'cross-site' }), true;
  if (req.headers.origin !== undefined && origin !== originOf(d.publicUrl) && !desktop) return send(res, 403, { error: 'cross-site' }), true;
  return false;
}

/** Handles one `/api/...` request. */
export async function handleApi(d: ApiDeps, req: IncomingMessage, res: ServerResponse): Promise<void> {
  const now = (d.now ?? Date.now)();
  const origin = String(req.headers.origin ?? '');
  if (DESKTOP_ORIGINS.includes(origin)) {
    res.setHeader('Access-Control-Allow-Origin', origin);
    res.setHeader('Access-Control-Allow-Headers', 'Authorization, Content-Type, X-Cave-Signup, X-Cave-Device');
    res.setHeader('Access-Control-Allow-Methods', 'GET, POST, PATCH, PUT, DELETE');
    res.setHeader('Vary', 'Origin');
  }
  if (req.method === 'OPTIONS') return void res.writeHead(204).end();
  const url = new URL(req.url ?? '/', 'http://x');
  const path = url.pathname;
  const method = req.method ?? 'GET';
  if (refusedCrossSite(d, req, res, method, origin, path)) return;

  try {
    // Read-only and public: the same for everyone, and a GET, so the cross-site checks above let it through.
    if (path === '/api/desktop-config' && method === 'GET') {
      if (!d.desktopConfig) return send(res, 404, { error: 'not-found' });
      const conf = await d.desktopConfig();
      return conf ? send(res, 200, conf) : send(res, 502, { error: 'unavailable' });
    }
    if (path === '/api/desktop-token' && method === 'POST') {
      if (!d.desktopToken) return send(res, 404, { error: 'not-found' });
      const out = await d.desktopToken((await readJson(req)) as Record<string, string>);
      return send(res, out.status, out.body);
    }
  } catch (e) {
    if (e instanceof SyntaxError) return sendBadBody(res, e);
    throw e;
  }

  const who = await d.whoIs(req as unknown as RequestLike);
  if (!who) return send(res, 401, { error: 'signed-out' });
  const token = String(req.headers['x-cave-signup'] ?? '') || undefined;
  const r = M.resolve(d.db, who, { adminEmail: d.adminEmail, now, token });
  const via = d.tailnet ? { via: 'tailnet' } : {};
  if (r.status !== 'member') return path === '/api/me' && method === 'GET' ? send(res, 200, { ...r, ...via }) : send(res, 403, { error: r.status });
  const me = r.member;

  try {
    if (path === '/api/devices' || path === '/api/caves' || path.startsWith('/api/caves/')) return await handleRooms(d.db, me.id, req, res, now);
    if (path === '/api/me' && method === 'GET') {
      return send(res, 200, { status: 'member', member: me, drives: M.listDrives(d.db, me.id), firstVisit: r.firstVisit, noticeSeen: M.noticeSeen(d.db, me.id), signups: getSetting(d.db, 'signups'), ...via, pass: signPass(d.passKey, { memberId: me.id, email: me.email, role: me.role }, now) });
    }
    if (path === '/api/me' && method === 'PATCH') {
      const b = await readJson(req);
      if (b.welcomed === true) M.markWelcomed(d.db, me.id, now);
      if (b.noticeSeen === true) M.markNoticeSeen(d.db, me.id);
      return send(res, 200, { member: M.updateProfile(d.db, me.id, { displayName: b.displayName as string | undefined, colour: b.colour as string | undefined }) });
    }
    if (path === '/api/me/drives' && method === 'POST') {
      const b = await readJson(req);
      return send(res, 200, { drive: M.addDrive(d.db, me.id, b.provider as M.DriveProvider, String(b.accountLabel ?? ''), now) });
    }
    // Every current member's name and email, for an owner picking guests for a cave's room (cave owners only).
    if (path === '/api/members/directory' && method === 'GET') {
      if (!ownsAnyCave(d.db, me.id)) return send(res, 403, { error: 'not-owner' });
      return send(res, 200, { members: M.listMembers(d.db).map((m) => ({ id: m.id, name: m.displayName, email: m.email })) });
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
    if (e instanceof SyntaxError) return sendBadBody(res, e);
    throw e;
  }
}

const esc = (t: string) => t.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
// The page someone gets instead of the app: plain, self-contained, nothing of the app in it.
function refusal(d: ApiDeps, res: ServerResponse, status: number, title: string, text: string): void {
  const out = d.signoutUrl ? `<p><a href="${esc(d.signoutUrl)}">Use another account</a></p>` : '';
  res.writeHead(status, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store' }).end(`<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><title>CaveCAD</title>
<style>:root{color-scheme:light dark}body{font:16px/1.5 system-ui,sans-serif;max-width:32rem;margin:4rem auto;padding:0 16px}</style></head>
<body><h1>${esc(title)}</h1><p>${text}</p>${out}<p><a href="/privacy.html">Privacy</a></p></body></html>`);
}

/**
 * Serves the web app, but only to someone who may use it (`M.mayLoadApp`): a stranger who signed in with
 * Google gets a short page saying CaveCAD is invite only, and none of the app. The privacy page is for anyone.
 */
export async function serveApp(d: ApiDeps, dist: string, req: IncomingMessage, res: ServerResponse): Promise<void> {
  if ((req.url ?? '/').split('?')[0] === '/privacy.html') return serveStatic(dist, req, res);
  const who = await d.whoIs(req as unknown as RequestLike);
  if (!who) return refusal(d, res, 401, 'Sign in to CaveCAD', 'Open <a href="https://cave.schonegg.stream/">cave.schonegg.stream</a> to sign in.');
  const may = M.mayLoadApp(d.db, who, { adminEmail: d.adminEmail, now: (d.now ?? Date.now)() });
  if (may === 'removed') return refusal(d, res, 403, 'Your CaveCAD account was removed', `You're signed in as ${esc(who.email)}. Ask an admin if this is a mistake.`);
  if (may === 'not-invited') return refusal(d, res, 403, 'CaveCAD is invite only for now', `You're signed in as ${esc(who.email)}, which hasn't been invited. If you were invited at another address, sign in with that one.`);
  return serveStatic(dist, req, res);
}
