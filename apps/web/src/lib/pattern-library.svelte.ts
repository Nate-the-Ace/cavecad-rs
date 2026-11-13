// The signed-in person's pattern library: linetypes, fills and motifs kept with their CaveCAD account, so they are there in
// every cave and on every device. Every pattern saved in a cave is also sent here (Nathan's call: automatically). A library
// pattern is put into a cave with "Use in this cave" (the cave keeps its own copy, so everyone who opens the cave sees it).
// Sharing makes a link to a snapshot; anyone signed in who opens it can add copies to their own library.
import { engine } from './engine';
import { copyName, plain, type FillDef, type LineDef, type Motif, type PatternKind } from './patterns';
import { serverAccess } from './room.svelte';

export type LibraryDef = LineDef | FillDef | Motif;
export type LibraryPattern = { kind: PatternKind; name: string; def: LibraryDef; updatedAt: number };

export const library = $state({
  items: [] as LibraryPattern[],
  /** True once the library has been read from the server (false when signed out or offline). */
  loaded: false,
  /** Why the last library action did not work, or ''. */
  problem: '',
  /** A share link someone opened, waiting to be added: who shared it and what is in it. */
  incoming: null as null | { code: string; from: string; items: { kind: PatternKind; name: string; def: LibraryDef }[] },
});

async function call(method: string, path: string, body?: unknown): Promise<Record<string, unknown> | null> {
  const a = await serverAccess();
  if (!a) {
    library.problem = 'Sign in to keep patterns in your account.';
    return null;
  }
  try {
    const res = await a.fetch(a.base + path, {
      method,
      headers: { ...(body ? { 'Content-Type': 'application/json' } : {}), ...(a.headers?.() ?? {}) },
      body: body ? JSON.stringify(body) : undefined,
      credentials: 'same-origin',
    });
    if (!res.ok) {
      library.problem = res.status === 409 ? 'Your library is full (500 patterns).' : res.status === 404 ? 'That pattern or link was not found.' : 'The library could not be reached. Try again in a moment.';
      return null;
    }
    library.problem = '';
    return (await res.json()) as Record<string, unknown>;
  } catch {
    library.problem = 'Can’t reach the CaveCAD server. Check you are online.';
    return null;
  }
}

/** Reads the library from the server. */
export async function refreshLibrary(): Promise<void> {
  const got = await call('GET', '/api/patterns');
  if (!got) return;
  library.items = got.patterns as LibraryPattern[];
  library.loaded = true;
}

/** Keeps a pattern in the library (sent after every save in a cave). Quiet when signed out. */
export async function keepInLibrary(kind: PatternKind, def: LibraryDef): Promise<void> {
  if (!(await serverAccess())) return;
  const d = plain(def);
  const got = await call('PUT', '/api/patterns', { kind, name: d.name, def: d });
  if (!got) return;
  const p = got.pattern as LibraryPattern;
  library.items = [...library.items.filter((i) => !(i.kind === p.kind && i.name === p.name)), p];
}

/** Takes a pattern out of the library (caves that use it keep their own copy). */
export async function dropFromLibrary(kind: PatternKind, name: string): Promise<void> {
  const got = await call('DELETE', `/api/patterns?kind=${encodeURIComponent(kind)}&name=${encodeURIComponent(name)}`);
  if (got) library.items = library.items.filter((i) => !(i.kind === kind && i.name === name));
}

/** The motifs a linetype or fill names. */
function motifsOf(kind: PatternKind, def: LibraryDef): string[] {
  if (kind === 'line') {
    const l = def as LineDef;
    return [...l.elements, ...(l.rows ?? []).flatMap((r) => r.elements)].flatMap((e) => (e.t === 'motif' ? [e.motif] : []));
  }
  if (kind === 'fill') return (def as FillDef).layers.flatMap((l) => (l.t === 'tile' ? [l.motif] : []));
  return [];
}

/**
 * Puts a library pattern into the open cave (and any of the library's motifs it needs that the cave does not have yet).
 * Gives back '' or why not.
 */
export function useInCave(item: LibraryPattern): string {
  const lib = engine.patternLibrary();
  for (const m of motifsOf(item.kind, item.def)) {
    if (lib.motifs.some((x) => x.name === m)) continue;
    const mine = library.items.find((i) => i.kind === 'motif' && i.name === m);
    if (mine) engine.savePattern('motif', plain(mine.def));
  }
  const r = engine.savePattern(item.kind, plain(item.def));
  return r === 'ok' ? '' : r.replace(/^error: /, '');
}

/** Makes a share link for some library patterns (with the motifs they use, when those are in the library). */
export async function shareLink(items: { kind: PatternKind; name: string }[]): Promise<string | null> {
  const all = [...items];
  for (const it of items) {
    const p = library.items.find((i) => i.kind === it.kind && i.name === it.name);
    for (const m of p ? motifsOf(p.kind, p.def) : [])
      if (library.items.some((i) => i.kind === 'motif' && i.name === m) && !all.some((a) => a.kind === 'motif' && a.name === m)) all.push({ kind: 'motif', name: m });
  }
  const got = await call('POST', '/api/patterns/share', { items: all });
  if (!got) return null;
  const u = new URL(location.href);
  u.search = '';
  u.hash = '';
  u.searchParams.set('patterns', String(got.code));
  return u.toString();
}

/** A share link opened in the address bar (?patterns=CODE): taken out of the address and read, to offer adding it. */
export async function checkShareLink(): Promise<void> {
  const u = new URL(location.href);
  const code = u.searchParams.get('patterns');
  if (!code) return;
  u.searchParams.delete('patterns');
  history.replaceState(history.state, '', u.toString());
  const got = await call('GET', `/api/patterns/share/${encodeURIComponent(code)}`);
  if (got) library.incoming = { code, from: String(got.from), items: got.items as { kind: PatternKind; name: string; def: LibraryDef }[] };
  else if (library.problem) library.problem = `The shared patterns could not be opened: ${library.problem}`;
}

/** Adds what a share holds to the library, as copies; a name already taken gets " copy". Gives back how many were added. */
export async function acceptShare(): Promise<number> {
  const inc = library.incoming;
  if (!inc) return 0;
  let n = 0;
  for (const it of inc.items) {
    const taken = library.items.filter((i) => i.kind === it.kind).map((i) => i.name);
    const name = taken.includes(it.name) ? copyName(it.name, taken) : it.name;
    const before = library.items.length;
    await keepInLibrary(it.kind, { ...plain(it.def), name } as LibraryDef);
    if (library.items.length > before || library.items.some((i) => i.kind === it.kind && i.name === name)) n++;
  }
  library.incoming = null;
  return n;
}
