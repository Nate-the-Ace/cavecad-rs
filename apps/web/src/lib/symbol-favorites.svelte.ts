// The user's favorite symbols as a small reactive store. This is a per-device preference kept in localStorage
// (it travels with the WebView's web storage in the native apps). It is deliberately NOT stored in the cave
// file: favorites are the user's preference, not survey data.
import { COLLAPSED_KEY, FAVORITES_KEY, isCollapsed, parseCollapsed, parseFavorites, serializeFavorites, toggleCollapsed, toggleKey } from './symbol-favorites';

function load(): string[] {
  try { return parseFavorites(localStorage.getItem(FAVORITES_KEY)); } catch { return []; }
}

export const symbolFavorites = $state<{ keys: string[] }>({ keys: load() });

export const isFavorite = (key: string): boolean => symbolFavorites.keys.includes(key);

export function toggleFavorite(key: string): void {
  symbolFavorites.keys = toggleKey(symbolFavorites.keys, key);
  try { localStorage.setItem(FAVORITES_KEY, serializeFavorites(symbolFavorites.keys)); } catch { /* storage unavailable: keep it for this session */ }
}

// Collapsed palette sections (same storage idea; `null` = untouched, so the phone can default to collapsed).
function loadCollapsed(): string[] | null {
  try { return parseCollapsed(localStorage.getItem(COLLAPSED_KEY)); } catch { return null; }
}
export const symbolSections = $state<{ collapsed: string[] | null }>({ collapsed: loadCollapsed() });

function saveCollapsed(): void {
  try { localStorage.setItem(COLLAPSED_KEY, JSON.stringify(symbolSections.collapsed)); } catch { /* storage unavailable */ }
}
export function sectionCollapsed(name: string, phone: boolean, searching: boolean): boolean {
  return isCollapsed(symbolSections.collapsed, name, phone, searching);
}
export function toggleSection(name: string, allNames: string[], phone: boolean): void {
  symbolSections.collapsed = toggleCollapsed(symbolSections.collapsed, name, allNames, phone);
  saveCollapsed();
}
/** Collapse every section (true) or expand every one (false). */
export function setAllSections(allNames: string[], collapse: boolean): void {
  symbolSections.collapsed = collapse ? [...allNames] : [];
  saveCollapsed();
}
