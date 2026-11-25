// Connects the phone's hardware action keys to the real survey. The Android shell dispatches
// `window` event `cave-hwkey` with detail { key: 'ptt'|'sos'|'yellow', phase: 'down'|'up' };
// `hardware-keys.ts` holds the mapping and its tests.

import { accept, laser, nextStation, survey, takeShot, undoLast } from './survey.svelte';
import { makeHwKeys, parseHwEvent } from './hardware-keys';

/** Starts listening for hardware keys. Returns the function that stops it. */
export function listenForHardwareKeys(): () => void {
  const onKey = makeHwKeys({
    active: () => survey.status === 'connected',
    laser: (on) => laser(on),
    fire: () => takeShot(),
    acceptNext: () => {
      accept();
      nextStation();
    },
    undo: () => void undoLast(),
  });
  const handler = (e: Event) => {
    const ev = parseHwEvent((e as CustomEvent).detail);
    if (ev) onKey(ev.key, ev.phase);
  };
  window.addEventListener('cave-hwkey', handler);
  return () => window.removeEventListener('cave-hwkey', handler);
}
