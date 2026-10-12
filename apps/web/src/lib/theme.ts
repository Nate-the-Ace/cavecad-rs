// The look of the whole app: dark or light (or follow the system) in a choice of colours.
// Ported from the old CaveCAD `Theme` (ten colours and a custom one, greys tinted by the colour's
// hue): the same presets and the same palette formulas, applied here as CSS variables on the page.
// A choice is always applied explicitly (never left to the browser's own guess), so text can't end
// up dark on dark when the page is shown inside something with a different theme.

export type Mode = 'dark' | 'light' | 'auto';

export interface Preset {
  id: string;
  name: string;
  /** Hue (0-360) the greys are tinted with. */
  hue: number;
  /** 1 is the standard strength of tint. */
  sat: number;
  /** The highlight colour. */
  accent: string;
}

/** The colours on offer (same as the old app). */
export const PRESETS: Preset[] = [
  { id: 'ocean', name: 'Ocean', hue: 210, sat: 1.0, accent: '#188cff' },
  { id: 'teal', name: 'Teal', hue: 178, sat: 1.0, accent: '#14b8a6' },
  { id: 'forest', name: 'Forest', hue: 145, sat: 0.9, accent: '#2fb86a' },
  { id: 'amber', name: 'Amber', hue: 38, sat: 0.9, accent: '#f0a020' },
  { id: 'ember', name: 'Ember', hue: 18, sat: 0.95, accent: '#f06a3a' },
  { id: 'crimson', name: 'Crimson', hue: 352, sat: 0.95, accent: '#e5484d' },
  { id: 'rose', name: 'Rose', hue: 328, sat: 0.95, accent: '#e255a1' },
  { id: 'violet', name: 'Violet', hue: 270, sat: 1.0, accent: '#8e6cef' },
  { id: 'indigo', name: 'Indigo', hue: 236, sat: 1.0, accent: '#5b6cf5' },
  { id: 'graphite', name: 'Graphite', hue: 215, sat: 0.15, accent: '#8aa0b8' },
];

export interface Choice {
  mode: Mode;
  /** A preset id, or `custom`. */
  id: string;
  /** The highlight of the custom theme, `#rrggbb`. */
  custom: string;
}

/** What people get until they choose (the old app's default). */
export const DEFAULT: Choice = { mode: 'dark', id: 'ocean', custom: '#188cff' };

const KEY = 'cavecad.theme';

/** `#rrggbb` from hue (0-360), saturation and lightness (0-1). */
export function hsl(h: number, s: number, l: number): string {
  s = Math.max(0, Math.min(1, s));
  l = Math.max(0, Math.min(1, l));
  const c = (1 - Math.abs(2 * l - 1)) * s;
  const hp = (((h % 360) + 360) % 360) / 60;
  const x = c * (1 - Math.abs((hp % 2) - 1));
  let [r, g, b] = [0, 0, 0];
  if (hp < 1) [r, g] = [c, x];
  else if (hp < 2) [r, g] = [x, c];
  else if (hp < 3) [g, b] = [c, x];
  else if (hp < 4) [g, b] = [x, c];
  else if (hp < 5) [r, b] = [x, c];
  else [r, b] = [c, x];
  const m = l - c / 2;
  const hex = (v: number) => Math.round((v + m) * 255).toString(16).padStart(2, '0');
  return `#${hex(r)}${hex(g)}${hex(b)}`;
}

/** Hue, saturation and lightness of `#rrggbb`. */
export function hexToHsl(hex: string): { h: number; s: number; l: number } {
  const r = parseInt(hex.slice(1, 3), 16) / 255;
  const g = parseInt(hex.slice(3, 5), 16) / 255;
  const b = parseInt(hex.slice(5, 7), 16) / 255;
  const mx = Math.max(r, g, b);
  const mn = Math.min(r, g, b);
  const l = (mx + mn) / 2;
  let h = 0;
  let s = 0;
  if (mx !== mn) {
    const d = mx - mn;
    s = l > 0.5 ? d / (2 - mx - mn) : d / (mx + mn);
    if (mx === r) h = (g - b) / d + (g < b ? 6 : 0);
    else if (mx === g) h = (b - r) / d + 2;
    else h = (r - g) / d + 4;
    h *= 60;
  }
  return { h, s, l };
}

/** The preset for an id; `custom` is made from the chosen highlight colour. */
export function preset(id: string, custom = DEFAULT.custom): Preset {
  if (id === 'custom') {
    const hex = /^#[0-9a-fA-F]{6}$/.test(custom) ? custom.toLowerCase() : DEFAULT.custom;
    const c = hexToHsl(hex);
    // the greys take the highlight's hue, as strongly as the colour is saturated
    return { id: 'custom', name: 'Custom', hue: c.h, sat: Math.max(0.12, Math.min(1, c.s * 1.05)), accent: hex };
  }
  return PRESETS.find((p) => p.id === id) ?? PRESETS[0];
}

/** The old app's palette for a mode and colour. */
export function palette(dark: boolean, p: Preset) {
  const k = p.sat;
  const H = p.hue;
  return dark
    ? {
        bg: hsl(H, 0.31 * k, 0.153), head: hsl(H, 0.3 * k, 0.12), tab: hsl(H, 0.32 * k, 0.19),
        line: hsl(H, 0.28 * k, 0.3), hover: hsl(H, 0.33 * k, 0.27), field: hsl(H, 0.3 * k, 0.1),
        dim: hsl(H, 0.18 * k, 0.55), dim2: hsl(H, 0.2 * k, 0.72),
        text: hsl(H, 0.9 * k, 0.96), strong: '#ffffff', accent: p.accent,
      }
    : {
        bg: hsl(H, 0.3 * k, 0.965), head: hsl(H, 0.25 * k, 0.92), tab: hsl(H, 0.22 * k, 0.86),
        line: hsl(H, 0.2 * k, 0.82), hover: hsl(H, 0.5 * k, 0.91), field: '#ffffff',
        dim: hsl(H, 0.15 * k, 0.64), dim2: hsl(H, 0.2 * k, 0.36),
        text: hsl(H, 0.4 * k, 0.15), strong: hsl(H, 0.5 * k, 0.09), accent: p.accent,
      };
}

/** The CSS variables the app's styles read, for a choice (`dark` resolves `auto`). */
export function cssVars(choice: Choice, dark: boolean): Record<string, string> {
  const c = palette(dark, preset(choice.id, choice.custom));
  return {
    '--bg': c.head, '--panel': c.bg, '--panel-2': c.tab, '--line': c.line, '--text': c.text,
    '--text-strong': c.strong, '--muted': c.dim2, '--dim': c.dim, '--accent': c.accent,
    '--accent-bg': c.hover, '--hover': c.hover, '--field': c.field,
    '--danger': dark ? '#ff7b6b' : '#c2331f', '--warn': dark ? '#f0b24a' : '#a15c00',
  };
}

export function load(): Choice {
  try {
    const raw = JSON.parse(localStorage.getItem(KEY) ?? 'null');
    if (raw && (raw.mode === 'dark' || raw.mode === 'light' || raw.mode === 'auto')) {
      return { mode: raw.mode, id: typeof raw.id === 'string' ? raw.id : DEFAULT.id, custom: typeof raw.custom === 'string' ? raw.custom : DEFAULT.custom };
    }
  } catch {
    // nothing saved, or storage is unavailable
  }
  return { ...DEFAULT };
}

const listeners = new Set<(dark: boolean) => void>();
const systemDark = () => window.matchMedia?.('(prefers-color-scheme: dark)').matches ?? true;
let current: Choice = DEFAULT;

/** Whether the theme now in force is dark. */
export function isDark(): boolean {
  return current.mode === 'auto' ? systemDark() : current.mode === 'dark';
}

/** The choice now in force. */
export function currentChoice(): Choice {
  return current;
}

/** Applies a choice to the page now, and tells whoever draws their own colours. */
export function apply(choice: Choice) {
  current = choice;
  const dark = isDark();
  const root = document.documentElement;
  for (const [k, v] of Object.entries(cssVars(choice, dark))) root.style.setProperty(k, v);
  root.dataset.theme = dark ? 'dark' : 'light';
  root.style.colorScheme = dark ? 'dark' : 'light';
  for (const f of listeners) f(dark);
}

/** Saves a choice on this machine and applies it. */
export function choose(choice: Choice) {
  try {
    localStorage.setItem(KEY, JSON.stringify(choice));
  } catch {
    // private window: it lasts until the page closes
  }
  apply(choice);
}

/** Calls `f` with whether the theme is dark, now and after every change (a chosen one, or the system's while on auto). */
export function onTheme(f: (dark: boolean) => void): () => void {
  listeners.add(f);
  return () => listeners.delete(f);
}

/** Applies the saved choice and follows the system while the mode is auto. Call once, before the app mounts. */
export function startTheme() {
  apply(load());
  window.matchMedia?.('(prefers-color-scheme: dark)').addEventListener('change', () => {
    if (current.mode === 'auto') apply(current);
  });
}
