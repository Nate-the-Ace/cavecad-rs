// The phone Notebook's display choices (Settings > Notebook), kept per device in localStorage. Reactive so the
// kept-alive Notebook panel redraws as soon as the switch changes.
import { BACKSIGHTS_KEY, parseBacksights, serializeBacksights } from './notebook-display.ts';

function load(): boolean {
  try {
    return parseBacksights(localStorage.getItem(BACKSIGHTS_KEY));
  } catch {
    return true;
  }
}

export const notebookDisplay = $state({ backsights: load() });

/** Show or hide the backsight columns in the phone Notebook (the readings stay stored either way). */
export function setBacksightsShown(on: boolean): void {
  notebookDisplay.backsights = on;
  try {
    localStorage.setItem(BACKSIGHTS_KEY, serializeBacksights(on));
  } catch {
    /* private mode: the choice lasts until reload */
  }
}
