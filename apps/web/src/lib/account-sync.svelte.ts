// Settings that follow the person to every device they sign in on: their preferences (units, theme, drawing options, snaps,
// symbol standard, drawing scales, sheet set-up, compact ribbon, the cave shelf at startup) and the panel arrangement on
// screen. Each is kept in this browser as before; this file watches those keeps and sends them to the account, and on sign-in
// takes the account's copy when it is newer (Nathan's rule: the newer copy wins). Settings that belong to the hardware (draw
// with a finger, pen pressure, the pen button) stay on each device.
import { cave, setCompactRibbon, setUnits } from './cave.svelte';
import { apply as applyTheme, load as loadTheme } from './theme';
import { snap } from './snap.svelte';
import { sheet } from './sheet.svelte';
import { shelf } from './shelf.svelte';
import { DOCK_KEY, applyDock } from './workspace.svelte';

/** The preferences kept with the account, by their key in this browser. */
const SETTINGS = [
  'cavecad.units',
  'cavecad.theme',
  'cavecad.options',
  'cavecad.snap.v1',
  'cavecad.symbolStandard',
  'cavecad.customScales',
  'cavecad.sheet',
  'cavecad.sheet.paperChosen',
  'cavecad.ribbon.compact',
  'cavecad.shelf.startup',
];
/** Drawing options that belong to this device's hardware: never sent, never replaced. */
const DEVICE_OPTIONS = ['finger', 'pressure', 'penButtonErases'];

const get = (k: string): string | null => {
  try {
    return localStorage.getItem(k);
  } catch {
    return null;
  }
};
const set = (k: string, v: string | null) => {
  try {
    if (v === null) localStorage.removeItem(k);
    else localStorage.setItem(k, v);
  } catch {
    /* storage unavailable */
  }
};
const parse = (t: string | null): unknown => {
  try {
    return t ? JSON.parse(t) : null;
  } catch {
    return null;
  }
};

type Pref = { key: 'settings' | 'dock'; read(): unknown; write(v: unknown): void };

const settings: Pref = {
  key: 'settings',
  read() {
    const out: Record<string, string | null> = {};
    for (const k of SETTINGS) out[k] = get(k);
    const opts = parse(out['cavecad.options']) as Record<string, unknown> | null;
    if (opts) {
      for (const d of DEVICE_OPTIONS) delete opts[d];
      out['cavecad.options'] = JSON.stringify(opts);
    }
    return out;
  },
  write(v) {
    const vals = (v ?? {}) as Record<string, string | null>;
    for (const k of SETTINGS) {
      if (!(k in vals)) continue;
      if (k === 'cavecad.options') {
        // this device's hardware options stay as they are
        const mine = (parse(get(k)) ?? {}) as Record<string, unknown>;
        const theirs = (parse(vals[k]) ?? {}) as Record<string, unknown>;
        for (const d of DEVICE_OPTIONS) if (d in mine) theirs[d] = mine[d];
        set(k, JSON.stringify(theirs));
      } else set(k, vals[k]);
    }
    // put what came on screen now (the rest is read when its panel opens)
    setUnits(get('cavecad.units') === 'imperial' ? 'imperial' : 'metric');
    const opts = (parse(get('cavecad.options')) ?? {}) as Record<string, number | boolean>;
    for (const [k, val] of Object.entries(opts)) cave.options[k] = val;
    applyTheme(loadTheme());
    const sn = parse(get('cavecad.snap.v1')) as { on?: boolean; settings?: unknown; prefs?: unknown } | null;
    if (sn) Object.assign(snap, Object.fromEntries(Object.entries(sn).filter(([k]) => k in snap)));
    const sh = parse(get('cavecad.sheet')) as Record<string, unknown> | null;
    if (sh) Object.assign(sheet, Object.fromEntries(Object.entries(sh).filter(([k]) => k in sheet)));
    setCompactRibbon(get('cavecad.ribbon.compact') === '1');
    shelf.startup = get('cavecad.shelf.startup') !== '0';
  },
};

const dock: Pref = {
  key: 'dock',
  read: () => parse(get(DOCK_KEY)),
  write(v) {
    if (applyDock(v)) set(DOCK_KEY, JSON.stringify(v));
  },
};

const PREFS = [settings, dock];
const atKey = (p: Pref) => `cavecad.sync.${p.key}.at`;
const seen: Record<string, string> = {};

async function access() {
  const { serverAccess } = await import('./room.svelte');
  return serverAccess();
}

async function push(p: Pref): Promise<void> {
  const a = await access();
  if (!a) return;
  try {
    await a.fetch(`${a.base}/api/me/prefs/${p.key}`, {
      method: 'PUT',
      headers: { 'Content-Type': 'application/json', ...(a.headers?.() ?? {}) },
      body: JSON.stringify({ value: p.read(), updatedAt: Number(get(atKey(p)) ?? 0) || Date.now() }),
      credentials: 'same-origin',
    });
  } catch {
    /* offline: sent again at the next change or sign-in */
  }
}

/** Brings this device and the account into step (call on sign-in): the newer copy of each wins. */
export async function syncAccountSettings(): Promise<void> {
  const a = await access();
  if (!a) return;
  for (const p of PREFS) {
    try {
      const res = await a.fetch(`${a.base}/api/me/prefs/${p.key}`, { headers: a.headers?.() ?? {}, credentials: 'same-origin' });
      if (!res.ok) continue;
      const { pref } = (await res.json()) as { pref: { value: unknown; updatedAt: number } | null };
      const mine = Number(get(atKey(p)) ?? 0) || 0;
      if (pref && pref.updatedAt > mine) {
        p.write(pref.value);
        set(atKey(p), String(pref.updatedAt));
        seen[p.key] = JSON.stringify(p.read());
      } else if (!pref || mine > pref.updatedAt) {
        if (!mine) set(atKey(p), String(Date.now()));
        await push(p);
      }
    } catch {
      /* offline: this device keeps what it has */
    }
  }
}

let watching: ReturnType<typeof setInterval> | null = null;
/** Watches the settings kept in this browser and sends a change to the account a few seconds after it is made. */
export function startAccountSync(): () => void {
  for (const p of PREFS) seen[p.key] ??= JSON.stringify(p.read());
  const check = () => {
    for (const p of PREFS) {
      const now = JSON.stringify(p.read());
      if (now === seen[p.key]) continue;
      seen[p.key] = now;
      set(atKey(p), String(Date.now()));
      void push(p);
    }
  };
  watching = setInterval(check, 4000);
  const onHide = () => document.visibilityState === 'hidden' && check();
  document.addEventListener('visibilitychange', onHide);
  return () => {
    if (watching) clearInterval(watching);
    document.removeEventListener('visibilitychange', onHide);
  };
}
