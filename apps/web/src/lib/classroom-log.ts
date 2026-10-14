// The class's chat as the teacher keeps it: a plain-text transcript to read or file, and the helpers
// the chat drawer needs. Pure, so it is checked without a connection.

/** One line of the class's record: something said, a hand going up or down, or a notice. */
export interface ChatEntry {
  /** Its place in the class's record, counting from 1. */
  n: number;
  /** When (ms since 1970, by the relay's clock). */
  at: number;
  kind: 'say' | 'hand' | 'system';
  id?: string;
  name?: string;
  role?: string;
  text: string;
}

/** A raised hand: who, and since when. */
export interface Hand {
  id: string;
  name: string;
  since: number;
}

const two = (n: number) => String(n).padStart(2, '0');

/** `14:05` in the reader's own time zone. */
export function clock(at: number): string {
  const d = new Date(at);
  return `${two(d.getHours())}:${two(d.getMinutes())}`;
}

/** `2026-10-13` in the reader's own time zone. */
export function day(at: number): string {
  const d = new Date(at);
  return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}`;
}

/** How long a hand has been up, in words. */
export function waiting(since: number, now = Date.now()): string {
  const s = Math.max(0, Math.round((now - since) / 1000));
  return s < 60 ? `${s} s` : `${Math.floor(s / 60)} min`;
}

/** Takes in entries that may repeat (a reconnect sends the backlog again): keeps each number once, in order. */
export function mergeEntries(have: ChatEntry[], more: ChatEntry[]): ChatEntry[] {
  const seen = new Set(have.map((e) => e.n));
  const added = more.filter((e) => !seen.has(e.n));
  return added.length ? [...have, ...added].sort((a, b) => a.n - b.n) : have;
}

/** The whole record as text, one line per entry, with who the teacher is and when the class started. */
export function transcript(className: string, started: number, entries: ChatEntry[], teacher = ''): string {
  const lines = [`Class: ${className}`, ...(teacher ? [`Teacher: ${teacher}`] : []), `Started: ${day(started)} ${clock(started)}`, `Lines: ${entries.length}`, ''];
  for (const e of entries) {
    const who = e.name ?? '';
    const tag = e.role === 'teacher' ? ' (teacher)' : '';
    if (e.kind === 'say') lines.push(`${clock(e.at)}  ${who}${tag}: ${e.text}`);
    else if (e.kind === 'hand') lines.push(`${clock(e.at)}  * ${who} ${e.text}`);
    else if (e.name) lines.push(`${clock(e.at)}  -- ${who} ${e.text}`);
    else lines.push(`${clock(e.at)}  -- ${e.text}`);
  }
  return `${lines.join('\n')}\n`;
}

/** A safe file name for a class's transcript. */
export function transcriptName(className: string, started: number): string {
  const base = className.replace(/[^A-Za-z0-9._-]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 50) || 'class';
  return `${base}-${day(started)}.txt`;
}

/** The link a student opens to join, made from the page's own address. */
export function classLink(base: string, room: string, token: string): string {
  const u = new URL(base);
  return `${u.origin}${u.pathname}?class=${encodeURIComponent(room)}&invite=${encodeURIComponent(token)}`;
}

/** The link for a direct class: only the session id, so one link serves every student. A mailbox set on this machine (not
 *  built into every copy) travels with the link, so a student's copy can use it too. */
export function liveLink(base: string, session: string, mailbox?: { endpoint: string; key: string }): string {
  const u = new URL(base);
  const extra = mailbox ? `&mb=${encodeURIComponent(mailbox.endpoint)}&mk=${encodeURIComponent(mailbox.key)}` : '';
  return `${u.origin}${u.pathname}?live=${encodeURIComponent(session)}${extra}`;
}

/** The same join as a link that opens the desktop app (`cavecad://join?...`), if it is installed. */
export function desktopLink(session: string, mailbox?: { endpoint: string; key: string }): string {
  const extra = mailbox ? `&mb=${encodeURIComponent(mailbox.endpoint)}&mk=${encodeURIComponent(mailbox.key)}` : '';
  return `cavecad://join?live=${encodeURIComponent(session)}${extra}`;
}

/** Reads a direct class link's query: the session and the mailbox it names, if any; null when it is not one. */
export function readLiveLink(search: string): { session: string; mailbox: { endpoint: string; key: string } | null } | null {
  const q = new URLSearchParams(search);
  const session = q.get('live');
  if (!session) return null;
  const endpoint = q.get('mb');
  const key = q.get('mk');
  return { session, mailbox: endpoint && key ? { endpoint, key } : null };
}

/** Reads a class link's query: the class and the invite, or null when it is not one. */
export function readClassLink(search: string): { room: string; invite: string } | null {
  const q = new URLSearchParams(search);
  const room = q.get('class');
  return room ? { room, invite: q.get('invite') ?? '' } : null;
}

/** Reads what someone pasted as a join link: a whole address, just the part after the `?`, or only the session code.
 *  Null when it is none of those. */
export function readJoinLink(text: string): { session: string; mailbox: { endpoint: string; key: string } | null } | null {
  const t = text.trim();
  if (!t) return null;
  try {
    const found = readLiveLink(new URL(t).search);
    if (found) return found;
  } catch {
    /* not an address */
  }
  if (t.includes('live=')) return readLiveLink(t.slice(t.indexOf('?') >= 0 ? t.indexOf('?') : 0).replace(/^(?!\?)/, '?'));
  return /^[A-Za-z0-9]{12,40}$/.test(t) ? { session: t, mailbox: null } : null;
}
