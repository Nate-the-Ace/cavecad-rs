// Who gets into CaveCAD, and the rules admins work under. Plain functions over the database, so every
// rule is tested without a network. Errors are thrown as `Error(code)`; the API turns codes into answers.
import { createHash, randomBytes, randomUUID } from 'node:crypto';
import { getSetting, type Db } from './db.ts';

const INVITE_DAYS = 7;
const DAY = 86_400_000;
const COLOURS = ['#3b82f6', '#e5484d', '#2fb86a', '#f0a020', '#8e6cef', '#14b8c4', '#e0559a', '#7c8a3a'];
const PROVIDERS = ['gdrive', 'onedrive', 'dropbox', 'webdav'] as const;

export type Role = 'admin' | 'member';
export type DriveProvider = (typeof PROVIDERS)[number];
/** Who a request is, as the identity adapter found it. */
export type Identity = { email: string; name: string; provider: 'google' | 'microsoft' | null; subject: string };
export type Member = { id: string; email: string; displayName: string; colour: string; role: Role; provider: string | null; invitedBy: string | null; createdAt: number; lastSeenAt: number | null; welcomedAt: number | null };
export type Invite = { id: string; email: string; role: Role; invitedBy: string; createdAt: number; expiresAt: number };
export type Drive = { id: string; provider: DriveProvider; accountLabel: string; createdAt: number };
export type NotInvitedReason = 'none' | 'expired' | 'used' | 'cancelled' | 'other-email';
export type Resolved =
  | { status: 'member'; member: Member; firstVisit: boolean }
  | { status: 'not-invited'; email: string; reason: NotInvitedReason; inviter?: string }
  | { status: 'removed'; email: string };

type MemberRow = { id: string; email: string; display_name: string; colour: string; role: Role; provider: string | null; invited_by: string | null; created_at: number; last_seen_at: number | null; welcomed_at: number | null; removed_at: number | null };
type InviteRow = { id: string; email: string; role: Role; token_hash: string; invited_by: string; created_at: number; expires_at: number; used_at: number | null; cancelled_at: number | null };

const norm = (email: string) => email.trim().toLowerCase();
const cleanName = (n: string) => n.replace(/[\x00-\x1f\x7f]/g, ' ').trim();
const fail = (code: string): never => { throw new Error(code); };
/** The SHA-256 of an invite token, which is all the database keeps. */
export const hashToken = (token: string) => createHash('sha256').update(token).digest('hex');
const toMember = (r: MemberRow): Member => ({ id: r.id, email: r.email, displayName: r.display_name, colour: r.colour, role: r.role, provider: r.provider, invitedBy: r.invited_by, createdAt: r.created_at, lastSeenAt: r.last_seen_at, welcomedAt: r.welcomed_at });
const toInvite = (r: InviteRow): Invite => ({ id: r.id, email: r.email, role: r.role, invitedBy: r.invited_by, createdAt: r.created_at, expiresAt: r.expires_at });
const memberByEmail = (db: Db, email: string) => db.prepare('SELECT * FROM members WHERE email = ?').get(norm(email)) as MemberRow | undefined;
const memberById = (db: Db, id: string) => db.prepare('SELECT * FROM members WHERE id = ?').get(id) as MemberRow | undefined;
const nameOf = (db: Db, id: string) => memberById(db, id)?.display_name;
const activeAdmins = (db: Db) => (db.prepare("SELECT COUNT(*) AS n FROM members WHERE role = 'admin' AND removed_at IS NULL").get() as { n: number }).n;

function requireAdmin(db: Db, actorId: string): MemberRow {
  const a = memberById(db, actorId);
  if (!a || a.removed_at !== null || a.role !== 'admin') fail('not-admin');
  return a!;
}

function addMember(db: Db, who: Identity, role: Role, invitedBy: string | null, now: number): MemberRow {
  const existing = memberByEmail(db, who.email);
  if (existing) {
    db.prepare('UPDATE members SET removed_at = NULL, role = ?, invited_by = ?, welcomed_at = NULL, provider = COALESCE(?, provider) WHERE id = ?').run(role, invitedBy, who.provider, existing.id);
    return memberById(db, existing.id)!;
  }
  const id = randomUUID();
  const colour = COLOURS[[...norm(who.email)].reduce((n, c) => n + c.charCodeAt(0), 0) % COLOURS.length];
  db.prepare('INSERT INTO members (id, email, display_name, colour, role, provider, invited_by, created_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)')
    .run(id, norm(who.email), (cleanName(who.name || '') || who.email.split('@')[0]).slice(0, 40), colour, role, who.provider, invitedBy, now);
  return memberById(db, id)!;
}

/**
 * Decides whether `who` gets in. A live invite for their email (with or without its link token) makes
 * them a member; `token` is only used to say why a link did not work. An empty member list lets
 * `adminEmail` in as the first admin. Marks the member as seen.
 */
export function resolve(db: Db, who: Identity, opts: { adminEmail: string; now: number; token?: string }): Resolved {
  const email = norm(who.email);
  const { now } = opts;
  let row = memberByEmail(db, email);
  const live = db.prepare('SELECT * FROM invites WHERE email = ? AND used_at IS NULL AND cancelled_at IS NULL AND expires_at > ? ORDER BY created_at DESC').get(email, now) as InviteRow | undefined;

  if (row && row.removed_at !== null && !live) return { status: 'removed', email };
  if (!row || row.removed_at !== null) {
    const noMembers = (db.prepare('SELECT COUNT(*) AS n FROM members').get() as { n: number }).n === 0;
    if (noMembers && email === norm(opts.adminEmail)) row = addMember(db, who, 'admin', null, now);
    else if (live) {
      // One step: a crash must not leave a member made but the invite still waiting.
      db.exec('BEGIN');
      try {
        row = addMember(db, who, live.role, live.invited_by, now);
        db.prepare('UPDATE invites SET used_at = ? WHERE id = ?').run(now, live.id);
        db.exec('COMMIT');
      } catch (e) {
        db.exec('ROLLBACK');
        throw e;
      }
    } else if (getSetting(db, 'signups') === 'open') row = addMember(db, who, 'member', null, now);
    else return notInvited(db, email, opts.token, now);
  }
  db.prepare('UPDATE members SET last_seen_at = ? WHERE id = ?').run(now, row.id);
  const member = toMember(memberById(db, row.id)!);
  return { status: 'member', member, firstVisit: member.welcomedAt === null };
}

/**
 * Whether `who` may load the web app at all: a member, someone with an invite waiting for their email,
 * the first admin, or anyone while sign-ups are open. Read only: joining happens through `resolve`.
 */
export function mayLoadApp(db: Db, who: Identity, opts: { adminEmail: string; now: number }): 'yes' | 'not-invited' | 'removed' {
  const email = norm(who.email);
  const row = memberByEmail(db, email);
  if (row && row.removed_at === null) return 'yes';
  const live = db.prepare('SELECT 1 FROM invites WHERE email = ? AND used_at IS NULL AND cancelled_at IS NULL AND expires_at > ?').get(email, opts.now);
  if (live || getSetting(db, 'signups') === 'open') return 'yes';
  if (!row && email === norm(opts.adminEmail) && (db.prepare('SELECT COUNT(*) AS n FROM members').get() as { n: number }).n === 0) return 'yes';
  return row ? 'removed' : 'not-invited';
}

function notInvited(db: Db, email: string, token: string | undefined, now: number): Resolved {
  if (!token) return { status: 'not-invited', email, reason: 'none' };
  const inv = db.prepare('SELECT * FROM invites WHERE token_hash = ?').get(hashToken(token)) as InviteRow | undefined;
  if (!inv) return { status: 'not-invited', email, reason: 'none' };
  const reason: NotInvitedReason = inv.cancelled_at !== null ? 'cancelled' : inv.used_at !== null ? 'used' : inv.expires_at <= now ? 'expired' : inv.email !== email ? 'other-email' : 'none';
  return { status: 'not-invited', email, reason, inviter: nameOf(db, inv.invited_by) };
}

/**
 * The newest one-time notice. 1: "Where your caves live" (server caves, 2026-10-26), shown again to members who
 * were welcomed under the old note saying the server could not read their caves.
 */
export const CURRENT_NOTICE = 1;

/** Marks the first-sign-in page as seen; it carries the current notice's wording, so that is marked seen too. */
export function markWelcomed(db: Db, memberId: string, now: number): void {
  db.prepare('UPDATE members SET welcomed_at = ?, notice_version = MAX(notice_version, ?) WHERE id = ?').run(now, CURRENT_NOTICE, memberId);
}

/** Whether the member has seen the current one-time notice. */
export function noticeSeen(db: Db, memberId: string): boolean {
  const r = db.prepare('SELECT notice_version FROM members WHERE id = ?').get(memberId) as { notice_version: number } | undefined;
  return !!r && r.notice_version >= CURRENT_NOTICE;
}

/** Marks the current one-time notice as seen. */
export function markNoticeSeen(db: Db, memberId: string): void {
  db.prepare('UPDATE members SET notice_version = MAX(notice_version, ?) WHERE id = ?').run(CURRENT_NOTICE, memberId);
}

/** Changes a member's own display name and colour. */
export function updateProfile(db: Db, memberId: string, p: { displayName?: string; colour?: string }): Member {
  if (p.displayName !== undefined) {
    if (typeof p.displayName !== 'string') fail('bad-name');
    const n = cleanName(p.displayName).slice(0, 40);
    if (!n) fail('bad-name');
    db.prepare('UPDATE members SET display_name = ? WHERE id = ?').run(n, memberId);
  }
  if (p.colour !== undefined) {
    if (typeof p.colour !== 'string' || !/^#[0-9a-f]{6}$/i.test(p.colour)) fail('bad-colour');
    db.prepare('UPDATE members SET colour = ? WHERE id = ?').run(p.colour, memberId);
  }
  return toMember(memberById(db, memberId)!);
}

/** Every member who has not been removed, oldest first. */
export function listMembers(db: Db): Member[] {
  return (db.prepare('SELECT * FROM members WHERE removed_at IS NULL ORDER BY created_at').all() as MemberRow[]).map(toMember);
}

/** Changes another member's role. The last admin cannot be demoted. */
export function setRole(db: Db, actorId: string, memberId: string, role: Role): Member {
  requireAdmin(db, actorId);
  if (role !== 'admin' && role !== 'member') fail('bad-role');
  if (actorId === memberId) fail('self');
  const m = memberById(db, memberId);
  if (!m || m.removed_at !== null) fail('not-found');
  // Backstop: the actor is an active admin and a different person, so a target admin means two or more admins.
  if (m!.role === 'admin' && role === 'member' && activeAdmins(db) <= 1) fail('last-admin');
  db.prepare('UPDATE members SET role = ? WHERE id = ?').run(role, memberId);
  return toMember(memberById(db, memberId)!);
}

/** Removes another member: they can no longer sign in. Their drives record is forgotten. */
export function removeMember(db: Db, actorId: string, memberId: string, now: number): void {
  requireAdmin(db, actorId);
  if (actorId === memberId) fail('self');
  const m = memberById(db, memberId);
  if (!m || m.removed_at !== null) fail('not-found');
  // Backstop: the actor is an active admin and a different person, so a target admin means two or more admins.
  if (m!.role === 'admin' && activeAdmins(db) <= 1) fail('last-admin');
  db.prepare('UPDATE members SET removed_at = ? WHERE id = ?').run(now, memberId);
  db.prepare('DELETE FROM drives WHERE member_id = ?').run(memberId);
}

/** Makes an invite for `email`; gives back the invite and the one-time link token (never stored). */
export function createInvite(db: Db, actorId: string, email: string, role: Role, now: number): { invite: Invite; token: string } {
  requireAdmin(db, actorId);
  const e = norm(email);
  if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(e)) fail('bad-email');
  if (role !== 'admin' && role !== 'member') fail('bad-role');
  const m = memberByEmail(db, e);
  if (m && m.removed_at === null) fail('already-member');
  db.prepare('UPDATE invites SET cancelled_at = ? WHERE email = ? AND used_at IS NULL AND cancelled_at IS NULL').run(now, e);
  const token = randomBytes(32).toString('base64url');
  const id = randomUUID();
  db.prepare('INSERT INTO invites (id, email, role, token_hash, invited_by, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?)')
    .run(id, e, role, hashToken(token), actorId, now, now + INVITE_DAYS * DAY);
  return { invite: toInvite(db.prepare('SELECT * FROM invites WHERE id = ?').get(id) as InviteRow), token };
}

/** Invites still waiting to be used, newest first. */
export function listInvites(db: Db, now: number): Invite[] {
  return (db.prepare('SELECT * FROM invites WHERE used_at IS NULL AND cancelled_at IS NULL AND expires_at > ? ORDER BY created_at DESC').all(now) as InviteRow[]).map(toInvite);
}

/** Cancels a waiting invite. */
export function cancelInvite(db: Db, actorId: string, inviteId: string, now: number): void {
  requireAdmin(db, actorId);
  const r = db.prepare('UPDATE invites SET cancelled_at = ? WHERE id = ? AND used_at IS NULL AND cancelled_at IS NULL').run(now, inviteId);
  if (r.changes === 0) fail('not-found');
}

/** Replaces a waiting invite with a fresh one (new token, new 7 days) for the same email and role. */
export function resendInvite(db: Db, actorId: string, inviteId: string, now: number): { invite: Invite; token: string } {
  requireAdmin(db, actorId);
  const old = db.prepare('SELECT * FROM invites WHERE id = ? AND used_at IS NULL AND cancelled_at IS NULL').get(inviteId) as InviteRow | undefined;
  if (!old) fail('not-found');
  return createInvite(db, actorId, old!.email, old!.role, now);
}

/** The drives a member has connected. */
export function listDrives(db: Db, memberId: string): Drive[] {
  return (db.prepare('SELECT id, provider, account_label AS accountLabel, created_at AS createdAt FROM drives WHERE member_id = ? ORDER BY created_at').all(memberId) as Drive[]);
}

/** Records a drive the member connected (which provider and whose account; never a key). */
export function addDrive(db: Db, memberId: string, provider: DriveProvider, accountLabel: string, now: number): Drive {
  if (!PROVIDERS.includes(provider)) fail('bad-provider');
  const label = typeof accountLabel === 'string' ? accountLabel : '';
  const id = randomUUID();
  db.prepare('INSERT INTO drives (id, member_id, provider, account_label, created_at) VALUES (?, ?, ?, ?, ?)').run(id, memberId, provider, label.slice(0, 200), now);
  return { id, provider, accountLabel: label.slice(0, 200), createdAt: now };
}

/** Forgets one of the member's own drives. */
export function removeDrive(db: Db, memberId: string, driveId: string): void {
  const r = db.prepare('DELETE FROM drives WHERE id = ? AND member_id = ?').run(driveId, memberId);
  if (r.changes === 0) fail('not-found');
}
