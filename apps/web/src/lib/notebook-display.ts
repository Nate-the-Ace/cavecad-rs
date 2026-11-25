// How the phone Notebook is drawn, kept per device (pure helpers; the reactive store is notebook-display.svelte.ts).
export const BACKSIGHTS_KEY = 'cavecad.notebook.backsights';

/** The Survey toggle's own choice (ask for a backsight after each leg), kept per device. */
export const SURVEY_BACKSIGHTS_KEY = 'cavecad.survey.backsights';

/** The stored Survey choice as a flag: only an explicit "1" turns collection on (off when nothing usable is stored). */
export const parseSurveyBacksights = (raw: string | null | undefined): boolean => raw === '1';

/** The Survey choice kept on this device (off when none, or when storage is not available). */
export function readSurveyBacksights(): boolean {
  try {
    return parseSurveyBacksights(localStorage.getItem(SURVEY_BACKSIGHTS_KEY));
  } catch {
    return false;
  }
}

/** The stored visibility as a flag: an explicit "0" or "1" wins; with nothing usable stored it starts as `fallback`
 * (the Survey's backsights choice), so the two settings agree on a fresh start. */
export const parseBacksights = (raw: string | null | undefined, fallback: boolean): boolean => (raw === '0' ? false : raw === '1' ? true : fallback);

/** The flag as the text kept in localStorage. */
export const serializeBacksights = (on: boolean): string => (on ? '1' : '0');

/** The Survey toggle changes data collection (ask for a backsight after each leg) and the Notebook's visibility together. */
export const surveyBacksightsChange = (on: boolean): { collect: boolean; show: boolean } => ({ collect: on, show: on });

/** The Notebook toggle changes visibility only: collection is left as it is (null). */
export const notebookBacksightsChange = (on: boolean): { collect: boolean | null; show: boolean } => ({ collect: null, show: on });
