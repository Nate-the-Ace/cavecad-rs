// Per-device phone preferences set in Settings > Sketch: the symbol standard the phone picker shows (the same stored
// key the desktop Symbols panel reads) and whether the Sketch screen shows the live shot strip at all. The strip's
// own fold state is separate (LiveStrip keeps it). The dock's Map panel shows the same strip under the same switch
// (user window > Options > Survey there).
import { readStandard, STANDARD_KEY, type Standard } from './symbol-palette';

export const LIVE_STRIP_KEY = 'cavecad.phone.liveStripEnabled';

/** The stored live-strip switch: on unless it was turned off. */
export const parseLiveStrip = (stored: string | null | undefined): boolean => stored !== 'off';

function loadLiveStrip(): boolean {
  try {
    return parseLiveStrip(localStorage.getItem(LIVE_STRIP_KEY));
  } catch {
    return true;
  }
}

export const phonePrefs = $state<{ standard: Standard; liveStrip: boolean }>({ standard: readStandard(), liveStrip: loadLiveStrip() });

export function setSymbolStandard(s: Standard): void {
  phonePrefs.standard = s;
  try {
    localStorage.setItem(STANDARD_KEY, s);
  } catch {
    /* just not remembered */
  }
}

export function setLiveStrip(on: boolean): void {
  phonePrefs.liveStrip = on;
  try {
    localStorage.setItem(LIVE_STRIP_KEY, on ? 'on' : 'off');
  } catch {
    /* just not remembered */
  }
}
