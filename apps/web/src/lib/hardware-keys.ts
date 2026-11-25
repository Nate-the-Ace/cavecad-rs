// Hardware action keys (Sonim XP8: PTT, SOS, Yellow) driving the in-cave survey. The native Android shell
// (apps/android) catches each key's Sonim broadcast and sends the page a `cave-hwkey` event: detail { key, phase }.
// This file is the pure mapping from a key to a survey action, with the actions passed in so it runs in node
// tests; the wiring to the real survey is in `hardware-keys-wire.ts`.

/** The keys the shell reports. */
export type HwKey = 'ptt' | 'sos' | 'yellow';
/** A key going down, or coming back up (SOS reports both; PTT and Yellow report both too). */
export type HwPhase = 'down' | 'up';

/** What a key can do to the survey. Each is an existing survey action. */
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
}

/** The same key arriving again inside this many milliseconds is a bounce (or a repeat), not a new press. */
export const HW_BOUNCE_MS = 150;
/** PTT held longer than this is a cancel: the laser goes off and no shot is taken. */
export const HW_CANCEL_MS = 1000;

const KEYS: readonly string[] = ['ptt', 'sos', 'yellow'];
const PHASES: readonly string[] = ['down', 'up'];

/** Reads a `cave-hwkey` event's detail; undefined when it is not a known key and phase. */
export function parseHwEvent(detail: unknown): { key: HwKey; phase: HwPhase } | undefined {
  if (!detail || typeof detail !== 'object') return undefined;
  const { key, phase } = detail as { key?: unknown; phase?: unknown };
  if (typeof key !== 'string' || typeof phase !== 'string') return undefined;
  if (!KEYS.includes(key) || !PHASES.includes(phase)) return undefined;
  return { key: key as HwKey, phase: phase as HwPhase };
}

/**
 * Makes the handler for hardware keys. `now` is the clock (milliseconds), passed in so a test can drive it.
 * Mapping: PTT is a trigger (down aims the laser, a quick release fires, a hold of HW_CANCEL_MS or more cancels);
 * Yellow accepts and goes to the next station; SOS undoes the last shot.
 */
export function makeHwKeys(actions: HwActions, now: () => number = () => Date.now()) {
  const lastDown = new Map<HwKey, number>();
  let pttDownAt: number | undefined; // set while PTT is held and aiming
  return function onKey(key: HwKey, phase: HwPhase) {
    if (key === 'ptt' && phase === 'up') {
      const at = pttDownAt;
      pttDownAt = undefined;
      if (at === undefined) return; // not a press we saw go down
      // the laser must never be left on, whether or not the survey is still live
      if (!actions.active() || now() - at >= HW_CANCEL_MS) return void actions.laser(false);
      return void actions.fire();
    }
    if (phase === 'up' || !actions.active()) return;
    const t = now();
    const before = lastDown.get(key);
    if (before !== undefined && t - before < HW_BOUNCE_MS) return;
    lastDown.set(key, t);
    switch (key) {
      case 'ptt': pttDownAt = t; actions.laser(true); break;
      case 'yellow': actions.acceptNext(); break;
      case 'sos': actions.undo(); break;
    }
  };
}
