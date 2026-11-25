// Shared by the desktop SymbolsPanel and the phone symbol picker: filtering, grouping by category, and the
// tile picture. Pure helpers plus one Svelte action; no state of their own.
import { drawSymbol, type SymbolDef } from './symbols';
import { onTheme } from './theme';
import { favoriteItems } from './symbol-favorites';

export interface SymbolGroup { name: string; items: SymbolDef[] }

/** Symbols whose name, UIS name or category contains the query (all of them for an empty query). */
export function filterSymbols(all: SymbolDef[], query: string): SymbolDef[] {
  const q = query.trim().toLowerCase();
  return q ? all.filter((s) => [s.name, s.uis, s.category].some((t) => t.toLowerCase().includes(q))) : all;
}

/** Files symbols under their category headings, in first-seen order. */
export function groupSymbols(shown: SymbolDef[]): SymbolGroup[] {
  const out: SymbolGroup[] = [];
  for (const s of shown) {
    let g = out.find((x) => x.name === s.category);
    if (!g) out.push((g = { name: s.category, items: [] }));
    g.items.push(s);
  }
  return out;
}

/** The tooltip / long name of a symbol, with its UIS name and set when they differ. */
export function symbolTitle(s: SymbolDef): string {
  return `${s.name}${s.uis !== s.name ? ` (UIS: ${s.uis})` : ''}${s.set !== 'nss' ? ` · ${s.set.toUpperCase()} set` : ''}`;
}

/** Svelte action: draws a symbol's picture into its tile canvas (44px square), following the theme. */
export function thumb(canvas: HTMLCanvasElement, def: SymbolDef) {
  const paint = () => {
    const size = 44;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = size * dpr;
    canvas.height = size * dpr;
    const g = canvas.getContext('2d')!;
    g.scale(dpr, dpr);
    g.clearRect(0, 0, size, size);
    g.strokeStyle = getComputedStyle(canvas).color;
    g.lineWidth = 1.4;
    g.lineCap = 'round';
    g.lineJoin = 'round';
    drawSymbol(g, def.key, [size / 2, size / 2], [size / 2, size / 2 - 18]);
  };
  paint();
  const off = onTheme(paint);
  return { destroy: off };
}

/**
 * The palette's sections: a "Favorites" section first (the favorites that match the search, in the order the
 * user favorited them; omitted when none do), then the usual categories. A favorite also stays in its category.
 */
export function paletteGroups(all: SymbolDef[], query: string, favoriteKeys: readonly string[]): SymbolGroup[] {
  const groups = groupSymbols(filterSymbols(all, query));
  const fav = filterSymbols(favoriteItems(all, favoriteKeys), query);
  return fav.length ? [{ name: 'Favorites', items: fav }, ...groups] : groups;
}

// Two symbol standards are carried: NSS (the US one) and UIS (the international one). The choice is a per-device setting.
export const STANDARD_KEY = 'cavecad.symbolStandard';
export type Standard = 'nss' | 'uis';

/** The stored standard (NSS when nothing is stored or storage is unavailable). */
export function readStandard(): Standard {
  try {
    return localStorage.getItem(STANDARD_KEY) === 'uis' ? 'uis' : 'nss';
  } catch {
    return 'nss';
  }
}

/** A symbol's name without its qualifier, so "Pit (depth mark)" and "Pit" are the same symbol. */
const base = (s: SymbolDef) => s.name.replace(/ \(.*\)$/, '').toLowerCase();

/**
 * What the palette shows: a search looks through both standards; otherwise only the chosen standard's version of a
 * shared symbol is shown (a symbol found only in the other standard still is).
 */
export function visibleSymbols(all: SymbolDef[], query: string, standard: Standard): SymbolDef[] {
  if (query.trim()) return filterSymbols(all, query);
  const preferred = new Set(all.filter((s) => s.set === standard).map(base));
  return all.filter((s) => s.set === standard || (s.set !== 'nss' && s.set !== 'uis') || !preferred.has(base(s)));
}
