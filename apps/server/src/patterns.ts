// A member's pattern library: the linetypes, fills and motifs they made, kept with their account so they are there in every
// cave and on every device. Every pattern saved in any cave lands here too (the app sends it). Sharing hands out a link to a
// snapshot of some patterns; any signed-in member who opens it can add copies to their own library (later edits by the sharer
// do not reach them). The definitions are the app's own JSON, checked here only for size and shape, never interpreted.
import { randomBytes } from 'node:crypto';
import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Db } from './db.ts';
import { readJson, send, sendBadBody } from './http.ts';

const KINDS = ['motif', 'line', 'fill'] as const;
type Kind = (typeof KINDS)[number];

/** Largest pattern definition kept (JSON text). */
export const MAX_DEF_BYTES = 64 * 1024;
/** Most patterns one member keeps. */
export const MAX_PATTERNS = 500;
/** Most patterns in one share. */
export const MAX_SHARED = 100;

export type LibraryPattern = { kind: Kind; name: string; def: unknown; updatedAt: number };

const fail = (code: string): never => {
  throw new Error(code);
};
const kindOf = (k: unknown): Kind => (KINDS.includes(k as Kind) ? (k as Kind) : fail('bad-kind'));
const nameOf = (n: unknown): string => {
  if (typeof n !== 'string') return fail('bad-name');
  const s = n.normalize('NFC').trim();
  if (!s || s.length > 40 || /[\u0000-\u001f]/.test(s)) fail('bad-name');
  return s;
};
const defOf = (d: unknown, name: string): string => {
  if (!d || typeof d !== 'object' || Array.isArray(d)) fail('bad-def');
  const text = JSON.stringify({ ...(d as object), name });
  if (text.length > MAX_DEF_BYTES) fail('too-big');
  return text;
};

/** The member's whole library. */
export function listPatterns(db: Db, memberId: string): LibraryPattern[] {
  return (
    db.prepare('SELECT kind, name, def, updated_at FROM member_patterns WHERE member_id = ? ORDER BY kind, name').all(memberId) as {
      kind: Kind;
      name: string;
      def: string;
      updated_at: number;
    }[]
  ).map((r) => ({ kind: r.kind, name: r.name, def: JSON.parse(r.def), updatedAt: r.updated_at }));
}

/** Keeps (or replaces) one pattern in the member's library. */
export function putPattern(db: Db, memberId: string, rawKind: unknown, rawName: unknown, rawDef: unknown, now: number): LibraryPattern {
  const kind = kindOf(rawKind);
  const name = nameOf(rawName);
  const def = defOf(rawDef, name);
  const exists = db.prepare('SELECT 1 FROM member_patterns WHERE member_id = ? AND kind = ? AND name = ?').get(memberId, kind, name);
  if (!exists) {
    const n = (db.prepare('SELECT COUNT(*) AS n FROM member_patterns WHERE member_id = ?').get(memberId) as { n: number }).n;
    if (n >= MAX_PATTERNS) fail('too-many');
  }
  db.prepare(
    `INSERT INTO member_patterns (member_id, kind, name, def, updated_at) VALUES (?, ?, ?, ?, ?)
     ON CONFLICT(member_id, kind, name) DO UPDATE SET def = excluded.def, updated_at = excluded.updated_at`,
  ).run(memberId, kind, name, def, now);
  return { kind, name, def: JSON.parse(def), updatedAt: now };
}

/** Takes one pattern out of the member's library (no error if it was not there). */
export function removePattern(db: Db, memberId: string, rawKind: unknown, rawName: unknown): void {
  db.prepare('DELETE FROM member_patterns WHERE member_id = ? AND kind = ? AND name = ?').run(memberId, kindOf(rawKind), nameOf(rawName));
}

/** Makes a share of some of the member's patterns (a snapshot, by kind and name). Gives back its code. */
export function sharePatterns(db: Db, memberId: string, rawItems: unknown, now: number): string {
  if (!Array.isArray(rawItems) || !rawItems.length || rawItems.length > MAX_SHARED) return fail('bad-request');
  const items = rawItems.map((it) => {
    const kind = kindOf((it as { kind?: unknown })?.kind);
    const name = nameOf((it as { name?: unknown })?.name);
    const row = db.prepare('SELECT def FROM member_patterns WHERE member_id = ? AND kind = ? AND name = ?').get(memberId, kind, name) as
      | { def: string }
      | undefined;
    if (!row) fail('not-found');
    return { kind, name, def: JSON.parse(row!.def) };
  });
  const code = randomBytes(12).toString('base64url');
  db.prepare('INSERT INTO pattern_shares (code, member_id, items, created_at) VALUES (?, ?, ?, ?)').run(code, memberId, JSON.stringify(items), now);
  return code;
}

/** What a share holds, and who made it. */
export function readShare(db: Db, rawCode: unknown): { from: string; items: { kind: Kind; name: string; def: unknown }[] } {
  if (typeof rawCode !== 'string' || !/^[A-Za-z0-9_-]{8,40}$/.test(rawCode)) return fail('not-found');
  const row = db
    .prepare('SELECT s.items, m.display_name AS name FROM pattern_shares s JOIN members m ON m.id = s.member_id WHERE s.code = ?')
    .get(rawCode) as { items: string; name: string } | undefined;
  if (!row) return fail('not-found');
  return { from: row.name, items: JSON.parse(row.items) };
}

const STATUS: Record<string, number> = { 'bad-kind': 400, 'bad-name': 400, 'bad-def': 400, 'bad-request': 400, 'too-big': 413, 'too-many': 409, 'not-found': 404 };

/** /api/patterns (GET list, PUT one, DELETE one), /api/patterns/share (POST), /api/patterns/share/<code> (GET). */
export async function handlePatterns(db: Db, memberId: string, req: IncomingMessage, res: ServerResponse, path: string, query: URLSearchParams, now: number): Promise<void> {
  const method = req.method ?? 'GET';
  try {
    if (path === '/api/patterns' && method === 'GET') return send(res, 200, { patterns: listPatterns(db, memberId) });
    if (path === '/api/patterns' && method === 'PUT') {
      let b: Record<string, unknown>;
      try {
        b = await readJson(req, MAX_DEF_BYTES + 1024);
      } catch (e) {
        return sendBadBody(res, e);
      }
      return send(res, 200, { pattern: putPattern(db, memberId, b.kind, b.name, b.def, now) });
    }
    if (path === '/api/patterns' && method === 'DELETE') {
      removePattern(db, memberId, query.get('kind'), query.get('name'));
      return send(res, 200, { ok: true });
    }
    if (path === '/api/patterns/share' && method === 'POST') {
      let b: Record<string, unknown>;
      try {
        b = await readJson(req);
      } catch (e) {
        return sendBadBody(res, e);
      }
      return send(res, 200, { code: sharePatterns(db, memberId, b.items, now) });
    }
    const m = /^\/api\/patterns\/share\/([A-Za-z0-9_-]{8,40})$/.exec(path);
    if (m && method === 'GET') return send(res, 200, readShare(db, m[1]));
    return send(res, 404, { error: 'not-found' });
  } catch (e) {
    const code = e instanceof Error ? e.message : 'bad-request';
    return send(res, STATUS[code] ?? 500, { error: STATUS[code] ? code : 'server-error' });
  }
}
