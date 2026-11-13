// A member's settings that follow them to every device they sign in on: saved panel layouts, the panel arrangement on
// screen, and their preferences (units, theme, drawing options and the like). Each is one JSON value
// under a known key, with when it was last changed, so a device can tell whose copy is newer.
import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Db } from './db.ts';
import { readJson, send, sendBadBody } from './http.ts';

/** The settings kept for a member, and the largest value each may be (JSON text). */
const KEYS: Record<string, number> = { layouts: 256 * 1024, dock: 256 * 1024, settings: 64 * 1024 };

export type Pref = { value: unknown; updatedAt: number } | null;

const fail = (code: string): never => {
  throw new Error(code);
};

/** One setting, or null when the member has none. */
export function getPref(db: Db, memberId: string, key: string): Pref {
  if (!(key in KEYS)) fail('not-found');
  const row = db.prepare('SELECT value, updated_at FROM member_prefs WHERE member_id = ? AND key = ?').get(memberId, key) as
    | { value: string; updated_at: number }
    | undefined;
  return row ? { value: JSON.parse(row.value), updatedAt: row.updated_at } : null;
}

/** Keeps one setting; `updatedAt` is when the device changed it (an older copy than the one kept is ignored). */
export function putPref(db: Db, memberId: string, key: string, value: unknown, updatedAt: unknown, now: number): Pref {
  if (!(key in KEYS)) fail('not-found');
  const text = JSON.stringify(value ?? null);
  if (text.length > KEYS[key]) fail('too-big');
  const at = typeof updatedAt === 'number' && Number.isFinite(updatedAt) ? Math.min(updatedAt, now + 60_000) : now;
  const before = getPref(db, memberId, key);
  if (before && before.updatedAt > at) return before; // a newer copy is kept already
  db.prepare(
    `INSERT INTO member_prefs (member_id, key, value, updated_at) VALUES (?, ?, ?, ?)
     ON CONFLICT(member_id, key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at`,
  ).run(memberId, key, text, at);
  return { value: JSON.parse(text), updatedAt: at };
}

/** /api/me/prefs/<key>: GET it, PUT {value, updatedAt}. */
export async function handlePrefs(db: Db, memberId: string, req: IncomingMessage, res: ServerResponse, key: string, now: number): Promise<void> {
  try {
    if (req.method === 'GET') return send(res, 200, { pref: getPref(db, memberId, key) });
    if (req.method === 'PUT') {
      let b: Record<string, unknown>;
      try {
        b = await readJson(req, 300 * 1024);
      } catch (e) {
        return sendBadBody(res, e);
      }
      return send(res, 200, { pref: putPref(db, memberId, key, b.value, b.updatedAt, now) });
    }
    return send(res, 404, { error: 'not-found' });
  } catch (e) {
    const code = e instanceof Error ? e.message : '';
    return send(res, code === 'too-big' ? 413 : code === 'not-found' ? 404 : 500, { error: code === 'too-big' || code === 'not-found' ? code : 'server-error' });
  }
}
