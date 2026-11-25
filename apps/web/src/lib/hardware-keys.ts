// Hardware action keys of rugged phones (Sonim PTT / SOS / Yellow, camera keys, programmable keys) driving the in-cave
// survey. The Android app (apps/desktop/src-tauri/gen/android) turns whatever a phone sends for a key (a vendor
// broadcast or a key code) into a vendor-neutral key name and passes the page a `cave-hwkey` event: detail { key, phase }.
// This file is the pure part: the key names, what a key can be set to do, the bindings, and the handler that turns a key
// into a survey action, with the actions passed in so it runs in node tests. The state and wiring are in
// `hardware-keys.svelte.ts`; how to add a phone is in docs/ANDROID-TAURI.md ("Hardware keys").

/** Vendor-neutral names of the keys. Each phone's own keys are mapped onto these on the Android side (HardwareKeys.kt). */
export const HW_KEYS = [
  { id: 'ptt', label: 'PTT' },
  { id: 'sos', label: 'SOS (red)' },
  { id: 'prog1', label: 'Programmable 1 (Sonim Yellow)' },
  { id: 'prog2', label: 'Programmable 2' },
  { id: 'prog3', label: 'Programmable 3' },
  { id: 'camera', label: 'Camera' },
  { id: 'focus', label: 'Camera half-press' },
] as const;
/** A vendor-neutral key name. */
export type HwKey = (typeof HW_KEYS)[number]['id'];
/** A key going down, or coming back up. */
export type HwPhase = 'down' | 'up';

/** What a key can be set to do. */
export const HW_ACTIONS = [
  { id: 'none', label: 'Nothing' },
  { id: 'trigger', label: 'Trigger: hold to aim, release to fire, long hold cancels' },
  { id: 'shot', label: 'Take a shot' },
  { id: 'aim', label: 'Laser on while held' },
  { id: 'laser', label: 'Laser on / off' },
  { id: 'accept-next', label: 'Accept, next station' },
  { id: 'undo', label: 'Undo last shot' },
  { id: 'next', label: 'Next station' },
  { id: 'previous', label: 'Previous station' },
] as const;
/** What a key does. */
export type HwAction = (typeof HW_ACTIONS)[number]['id'];
/** Each key's action. */
export type HwBindings = Record<HwKey, HwAction>;

/** What each key does until the person changes it (the Sonim XP8's keys as set up and tried with an X1). */
export const DEFAULT_BINDINGS: HwBindings = { ptt: 'trigger', sos: 'undo', prog1: 'accept-next', prog2: 'none', prog3: 'none', camera: 'shot', focus: 'aim' };

const KEY_IDS: readonly string[] = HW_KEYS.map((k) => k.id);
const ACTION_IDS: readonly string[] = HW_ACTIONS.map((a) => a.id);

/** Bindings from saved text: what is valid is kept, everything else (a bad file, an unknown key or action) is the default. */
export function parseBindings(text: string | null | undefined): HwBindings {
  const out = { ...DEFAULT_BINDINGS };
  try {
    const o = JSON.parse(text ?? 'null');
    if (o && typeof o === 'object') for (const k of KEY_IDS) if (typeof o[k] === 'string' && ACTION_IDS.includes(o[k])) out[k as HwKey] = o[k] as HwAction;
  } catch {
    // unreadable: the defaults
  }
  return out;
}

/** What the person added for a phone this app does not know: a broadcast action or a key code, each given one of the key names. */
export interface HwCustom {
  actions: { action: string; key: HwKey; phase: HwPhase }[];
  keycodes: { code: number; key: HwKey }[];
}
export const EMPTY_CUSTOM: HwCustom = { actions: [], keycodes: [] };
/** Most entries of each kind (the native side stops reading there too). */
export const CUSTOM_LIMIT = 24;
const ACTION_NAME = /^[A-Za-z0-9._]{3,120}$/;

/** True for a broadcast action name (letters, digits, dots, underscores). */
export const validAction = (s: unknown): s is string => typeof s === 'string' && ACTION_NAME.test(s);

/** Custom keys from saved text or a form: valid entries only, no repeats, at most CUSTOM_LIMIT of each kind. */
export function parseCustom(text: string | null | undefined): HwCustom {
  const out: HwCustom = { actions: [], keycodes: [] };
  try {
    const o = JSON.parse(text ?? 'null');
    for (const a of Array.isArray(o?.actions) ? o.actions : []) {
      if (out.actions.length >= CUSTOM_LIMIT) break;
      if (validAction(a?.action) && KEY_IDS.includes(a.key) && (a.phase === 'down' || a.phase === 'up') && !out.actions.some((x) => x.action === a.action))
        out.actions.push({ action: a.action, key: a.key, phase: a.phase });
    }
    for (const c of Array.isArray(o?.keycodes) ? o.keycodes : []) {
      if (out.keycodes.length >= CUSTOM_LIMIT) break;
      if (Number.isInteger(c?.code) && c.code > 0 && c.code < 100000 && KEY_IDS.includes(c.key) && !out.keycodes.some((x) => x.code === c.code))
        out.keycodes.push({ code: c.code, key: c.key });
    }
  } catch {
    // unreadable: nothing custom
  }
  return out;
}

/** What the page is told: a known key, or (only while learning) a key the app does not know, by its codes. */
export type HwEvent = { key: HwKey; phase: HwPhase } | { key: 'unknown'; code: number; scan: number; phase: HwPhase };

/** Reads a `cave-hwkey` event's detail; undefined when it is not a known shape. */
export function parseHwEvent(detail: unknown): HwEvent | undefined {
  if (!detail || typeof detail !== 'object') return undefined;
  const { key, phase, code, scan } = detail as { key?: unknown; phase?: unknown; code?: unknown; scan?: unknown };
  if (phase !== 'down' && phase !== 'up') return undefined;
  if (key === 'unknown') return Number.isInteger(code) ? { key, code: code as number, scan: Number.isInteger(scan) ? (scan as number) : 0, phase } : undefined;
  return typeof key === 'string' && KEY_IDS.includes(key) ? { key: key as HwKey, phase } : undefined;
}

/** What the handler can do to the survey. Each is an existing survey action. */
export interface HwActions {
  /** True while the survey is live (an instrument or the simulator is connected). Keys do nothing otherwise. */
  active(): boolean;
  /** Switches the instrument's laser on or off, to aim. */
  laser(on: boolean): void;
  /** Takes the shot. */
  fire(): void;
  /** Accepts the shot waiting, then moves to the next station. */
  acceptNext(): void;
  /** Takes back the last thing written. */
  undo(): void;
  /** Moves to the next station. */
  next(): void;
  /** Moves to the previous station. */
  previous(): void;
}

/** The same key arriving again inside this many milliseconds is a bounce (or a repeat), not a new press. */
export const HW_BOUNCE_MS = 150;
/** A trigger held longer than this is a cancel: the laser goes off and no shot is taken. */
export const HW_CANCEL_MS = 1000;

/**
 * Makes the handler for hardware keys. `bindings` says what each key does now; `now` is the clock (milliseconds),
 * passed in so a test can drive it. A laser a key switched on is never left on: a release (or the survey ending) puts it out.
 */
export function makeHwKeys(actions: HwActions, bindings: () => HwBindings = () => DEFAULT_BINDINGS, now: () => number = () => Date.now()) {
  const lastDown = new Map<HwKey, number>();
  const held = new Map<HwKey, { at: number; kind: 'trigger' | 'aim' }>(); // keys holding the laser on
  let toggled = false;
  return function onKey(key: HwKey, phase: HwPhase) {
    if (phase === 'up') {
      const h = held.get(key);
      if (!h) return;
      held.delete(key);
      if (h.kind === 'aim' || !actions.active() || now() - h.at >= HW_CANCEL_MS) return void actions.laser(false);
      return void actions.fire();
    }
    if (!actions.active()) return;
    const t = now();
    const before = lastDown.get(key);
    if (before !== undefined && t - before < HW_BOUNCE_MS) return;
    lastDown.set(key, t);
    switch (bindings()[key]) {
      case 'trigger': held.set(key, { at: t, kind: 'trigger' }); actions.laser(true); break;
      case 'aim': held.set(key, { at: t, kind: 'aim' }); actions.laser(true); break;
      case 'shot': actions.fire(); break;
      case 'laser': toggled = !toggled; actions.laser(toggled); break;
      case 'accept-next': actions.acceptNext(); break;
      case 'undo': actions.undo(); break;
      case 'next': actions.next(); break;
      case 'previous': actions.previous(); break;
      case 'none': break;
    }
  };
}
