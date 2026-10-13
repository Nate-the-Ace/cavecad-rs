// The signaling mailbox: where two CaveCAD pages that have never met leave each other the short notes a direct
// (WebRTC) connection needs to start. It is CaveCAD's own Apps Script (tools/signal-mailbox) in production, an
// in-memory one in tests. The mailbox only ever carries those notes (a hello, an offer, an answer): the
// cave, the chat and the teacher's view go straight between the pages once they are connected.

/** One note in the mailbox. `name` is `hello:ID`, `offer:ID` or `answer:ID`, where ID is one student's attempt. */
export interface Note {
  name: string;
  value: string;
}

export interface Mailbox {
  /** Leaves a note (replacing one of the same name). Rejects with a sentence on failure. */
  put(session: string, name: string, value: string): Promise<void>;
  /** The note with this name, or null. */
  get(session: string, name: string): Promise<string | null>;
  /** The notes whose names start with `prefix`. */
  list(session: string, prefix: string): Promise<Note[]>;
  /** Takes a note away. */
  del(session: string, name: string): Promise<void>;
}

const WHY: Record<string, string> = {
  key: 'The class mailbox did not accept this copy of CaveCAD.',
  session: 'That class link is not valid.',
  name: 'The mailbox could not use that note.',
  'too-big': 'A connection note was too big for the mailbox.',
  busy: 'The class mailbox is busy. Try again in a minute.',
  empty: 'A connection note was empty.',
};

/** The mailbox on Apps Script (or anything that answers the same calls), reached with plain requests that need no permission step. */
export function httpMailbox(endpoint: string, key: string, fetcher: typeof fetch = fetch): Mailbox {
  const url = (op: string, session: string, extra: Record<string, string> = {}) => {
    const q = new URLSearchParams({ op, k: key, s: session, ...extra });
    return `${endpoint}${endpoint.includes('?') ? '&' : '?'}${q}`;
  };
  async function call(input: string, init?: RequestInit): Promise<{ value?: string | null; items?: { n: string; v: string }[] }> {
    let out: { ok?: boolean; error?: string; value?: string | null; items?: { n: string; v: string }[] } | null = null;
    try {
      const r = await fetcher(input, init);
      out = await r.json();
    } catch {
      throw new Error('Could not reach the class mailbox.');
    }
    if (!out?.ok) throw new Error(WHY[out?.error ?? ''] ?? `The class mailbox refused (${out?.error ?? 'no answer'}).`);
    return out;
  }
  const text = { method: 'POST', headers: { 'content-type': 'text/plain;charset=utf-8' } } as const;
  return {
    put: async (session, name, value) => void (await call(url('put', session, { n: name }), { ...text, body: value })),
    get: async (session, name) => (await call(url('get', session, { n: name }))).value ?? null,
    list: async (session, prefix) => ((await call(url('list', session, { p: prefix }))).items ?? []).map((i) => ({ name: i.n, value: i.v })),
    del: async (session, name) => void (await call(url('del', session, { n: name }), { ...text, body: '-' })),
  };
}

/** A mailbox in memory, for tests and for two pages in one process. */
export function memoryMailbox(): Mailbox {
  const notes = new Map<string, string>();
  const k = (s: string, n: string) => `${s}\u0000${n}`;
  return {
    put: async (s, n, v) => void notes.set(k(s, n), v),
    get: async (s, n) => notes.get(k(s, n)) ?? null,
    list: async (s, p) => [...notes].filter(([key]) => key.startsWith(`${s}\u0000${p}`)).map(([key, value]) => ({ name: key.slice(s.length + 1), value })),
    del: async (s, n) => void notes.delete(k(s, n)),
  };
}

const STORE_KEY = 'cavecad.signal';

/** What the mailbox is set to: the address (the Apps Script web app's /exec URL) and its key. */
export interface MailboxConfig {
  endpoint: string;
  key: string;
  /** Where it came from: set on this machine, or built into this copy (which every copy of the same build shares). */
  source: 'local' | 'build';
}

/** The mailbox this copy was built with, or one set on this machine (to try a new deployment without a rebuild). */
export function mailboxConfig(): MailboxConfig | null {
  try {
    const saved = JSON.parse(localStorage.getItem(STORE_KEY) ?? 'null') as Partial<MailboxConfig> | null;
    if (saved?.endpoint && saved.key) return { endpoint: saved.endpoint, key: saved.key, source: 'local' };
  } catch {
    /* nothing saved */
  }
  const endpoint = (import.meta.env?.VITE_SIGNAL_ENDPOINT as string | undefined)?.trim() ?? '';
  const key = (import.meta.env?.VITE_SIGNAL_KEY as string | undefined)?.trim() ?? '';
  return endpoint && key ? { endpoint, key, source: 'build' } : null;
}

/** Remembers a mailbox on this machine (or forgets it, when given null). */
export function saveMailboxConfig(c: Pick<MailboxConfig, 'endpoint' | 'key'> | null) {
  try {
    if (c && c.endpoint.trim() && c.key.trim()) localStorage.setItem(STORE_KEY, JSON.stringify({ endpoint: c.endpoint.trim(), key: c.key.trim() }));
    else localStorage.removeItem(STORE_KEY);
  } catch {
    /* not remembered */
  }
}

/** The mailbox to use, or null when this copy has none. */
export function configuredMailbox(): Mailbox | null {
  const c = mailboxConfig();
  return c ? httpMailbox(c.endpoint, c.key) : null;
}
