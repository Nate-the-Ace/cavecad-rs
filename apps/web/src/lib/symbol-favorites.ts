// Pure helpers for the user's favorite symbols (no Svelte, no storage), so they can be tested under plain node.
// A favorite list is an ordered array of symbol keys, oldest favorite first.

export const FAVORITES_KEY = 'cavecad.symbols.favorites.v1';

/** Reads stored text back into a key list; anything corrupt or of the wrong shape gives an empty list. */
export function parseFavorites(text: string | null | undefined): string[] {
  if (!text) return [];
  try {
    const v: unknown = JSON.parse(text);
    if (!Array.isArray(v)) return [];
    const out: string[] = [];
    for (const k of v) if (typeof k === 'string' && k && !out.includes(k)) out.push(k);
    return out;
  } catch {
    return [];
  }
}

export const serializeFavorites = (keys: readonly string[]): string => JSON.stringify(keys);

/** A new list with the key removed if present, else appended (so order is the order of favouriting). */
export function toggleKey(keys: readonly string[], key: string): string[] {
  return keys.includes(key) ? keys.filter((k) => k !== key) : [...keys, key];
}

/**
 * The favorites that exist in `all`, in favorited order. Keys with no symbol (removed in a newer build) are
 * skipped here, but toggleKey never drops them from the stored list.
 */
export function favoriteItems<T extends { key: string }>(all: readonly T[], keys: readonly string[]): T[] {
  const byKey = new Map(all.map((s) => [s.key, s] as const));
  const out: T[] = [];
  for (const k of keys) { const s = byKey.get(k); if (s) out.push(s); }
  return out;
}

// Collapsed palette sections, also a per-device preference. `null` means the user has not touched any section yet.
export const COLLAPSED_KEY = 'cavecad.symbols.collapsed.v1';
export const FAVORITES_SECTION = 'Favorites';

/** Stored text to a list of collapsed section names; null when nothing valid is stored (never touched). */
export function parseCollapsed(text: string | null | undefined): string[] | null {
  if (!text) return null;
  try {
    const v: unknown = JSON.parse(text);
    return Array.isArray(v) ? v.filter((k): k is string => typeof k === 'string') : null;
  } catch {
    return null;
  }
}

/**
 * Whether a section is collapsed. While searching, everything shows expanded. Untouched (null) state: nothing is
 * collapsed on desktop; on the phone every section except Favorites is.
 */
export function isCollapsed(stored: readonly string[] | null, name: string, phone: boolean, searching: boolean): boolean {
  if (searching) return false;
  if (stored) return stored.includes(name);
  return phone && name !== FAVORITES_SECTION;
}

/** The new stored list after toggling one section, starting from the defaults when nothing was stored yet. */
export function toggleCollapsed(stored: readonly string[] | null, name: string, allNames: readonly string[], phone: boolean): string[] {
  const base = stored ?? (phone ? allNames.filter((n) => n !== FAVORITES_SECTION) : []);
  return base.includes(name) ? base.filter((n) => n !== name) : [...base, name];
}
