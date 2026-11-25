// The state and wiring of the hardware keys (rugged phones' PTT, SOS, programmable and camera keys): what each key is set
// to do (kept on this device), the keys the person added for a phone this app does not know, learn mode, and the
// listener that passes each key to the survey. The pure part (names, actions, handler) is `hardware-keys.ts`; the
// native side is the Android app's `HardwareKeys.kt` and its `CaveKeys` bridge, so in a browser or on a phone without
// it `hw.native` stays null and the settings do not show.

import { accept, laser, nextStation, previousStation, survey, takeShot, undoLast } from './survey.svelte';
import {
  DEFAULT_BINDINGS, CUSTOM_LIMIT, makeHwKeys, parseBindings, parseCustom, parseHwEvent, validAction,
  type HwAction, type HwBindings, type HwKey, type HwPhase,
} from './hardware-keys';

const BINDINGS_KEY = 'cavecad.hwkeys.bindings';
const CUSTOM_KEY = 'cavecad.hwkeys.custom';

/** The Android app's door (`CaveKeys`, a JavaScript interface of its web view). */
interface Bridge {
  info(): string;
  configure(json: string): void;
  learn(on: boolean): void;
  setActive(on: boolean): void;
}
const bridge = (): Bridge | undefined => (window as unknown as { CaveKeys?: Bridge }).CaveKeys;

function read(key: string): string | null {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}
function write(key: string, value: string) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // private window: the setting lasts for this run only
  }
}

/** What the screens show. */
export const hw = $state({
  bindings: parseBindings(read(BINDINGS_KEY)) as HwBindings,
  custom: parseCustom(read(CUSTOM_KEY)),
  /** The phone, when this app can hear its keys (the Android app); null otherwise. */
  native: null as { manufacturer: string; model: string } | null,
  /** The last key heard, in words, for the settings. */
  last: '',
  /** Learn mode: a key the app does not know is reported (and kept from doing its usual job) so it can be given a name. */
  learning: false,
  /** The unknown key just pressed while learning. */
  unknown: null as { code: number; scan: number } | null,
});

/** Sets what a key does. */
export function setBinding(key: HwKey, action: HwAction) {
  hw.bindings = { ...hw.bindings, [key]: action };
  write(BINDINGS_KEY, JSON.stringify(hw.bindings));
}

/** Puts every key back to what it did at first. */
export function resetBindings() {
  hw.bindings = { ...DEFAULT_BINDINGS };
  write(BINDINGS_KEY, JSON.stringify(hw.bindings));
}

function keepCustom() {
  write(CUSTOM_KEY, JSON.stringify(hw.custom));
  bridge()?.configure(JSON.stringify(hw.custom));
}

/** Teaches the app a broadcast a phone sends for a key. False when the name is not a broadcast name, or it is known already, or the list is full. */
export function addAction(action: string, key: HwKey, phase: HwPhase): boolean {
  const a = action.trim();
  if (!validAction(a) || hw.custom.actions.length >= CUSTOM_LIMIT || hw.custom.actions.some((x) => x.action === a)) return false;
  hw.custom = { ...hw.custom, actions: [...hw.custom.actions, { action: a, key, phase }] };
  keepCustom();
  return true;
}

/** Teaches the app a key code (both its press and its release) as one of the key names. False when it is known already or the list is full. */
export function addKeycode(code: number, key: HwKey): boolean {
  if (!Number.isInteger(code) || code <= 0 || hw.custom.keycodes.length >= CUSTOM_LIMIT || hw.custom.keycodes.some((x) => x.code === code)) return false;
  hw.custom = { ...hw.custom, keycodes: [...hw.custom.keycodes, { code, key }] };
  keepCustom();
  return true;
}

/** Forgets a custom broadcast. */
export function removeAction(action: string) {
  hw.custom = { ...hw.custom, actions: hw.custom.actions.filter((x) => x.action !== action) };
  keepCustom();
}

/** Forgets a custom key code. */
export function removeKeycode(code: number) {
  hw.custom = { ...hw.custom, keycodes: hw.custom.keycodes.filter((x) => x.code !== code) };
  keepCustom();
}

/** Turns learn mode on or off. */
export function setLearning(on: boolean) {
  hw.learning = on;
  hw.unknown = null;
  bridge()?.learn(on);
}

/** Gives the key just pressed in learn mode one of the names, and keeps it. */
export function useUnknown(key: HwKey) {
  if (hw.unknown && addKeycode(hw.unknown.code, key)) hw.unknown = null;
}

/** Starts listening for hardware keys and tells the native side what the person added. Returns the function that stops it. */
export function listenForHardwareKeys(): () => void {
  const onKey = makeHwKeys(
    {
      active: () => survey.status === 'connected',
      laser: (on) => laser(on),
      fire: () => takeShot(),
      acceptNext: () => {
        accept();
        nextStation();
      },
      undo: () => void undoLast(),
      next: () => nextStation(),
      previous: () => previousStation(),
    },
    () => hw.bindings,
  );
  try {
    const b = bridge();
    if (b) {
      const info = JSON.parse(b.info());
      hw.native = { manufacturer: String(info.manufacturer ?? ''), model: String(info.model ?? '') };
      b.configure(JSON.stringify(hw.custom));
    }
  } catch {
    hw.native = null; // a bridge that does not answer: the settings stay hidden
  }
  const handler = (e: Event) => {
    const ev = parseHwEvent((e as CustomEvent).detail);
    if (!ev) return;
    if (ev.key === 'unknown') {
      hw.unknown = { code: ev.code, scan: ev.scan };
      hw.last = `key code ${ev.code}${ev.scan ? ` (scan ${ev.scan})` : ''}`;
      return;
    }
    hw.last = `${ev.key} ${ev.phase}`;
    onKey(ev.key, ev.phase);
  };
  window.addEventListener('cave-hwkey', handler);
  // the phone takes key events (a camera key) for the survey only while it is live; otherwise they keep their usual jobs
  const stopTelling = $effect.root(() => {
    $effect(() => {
      bridge()?.setActive(survey.status === 'connected');
    });
  });
  return () => {
    window.removeEventListener('cave-hwkey', handler);
    stopTelling();
  };
}
