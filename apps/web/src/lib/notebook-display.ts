// How the phone Notebook is drawn, kept per device (pure helpers; the reactive store is notebook-display.svelte.ts).
export const BACKSIGHTS_KEY = 'cavecad.notebook.backsights';

/** The stored text as a flag: only an explicit "0" hides the backsight columns, anything else (or nothing) shows them. */
export const parseBacksights = (raw: string | null | undefined): boolean => raw !== '0';

/** The flag as the text kept in localStorage. */
export const serializeBacksights = (on: boolean): string => (on ? '1' : '0');

/** The Survey toggle changes data collection (ask for a backsight after each leg) and the Notebook's visibility together. */
export const surveyBacksightsChange = (on: boolean): { collect: boolean; show: boolean } => ({ collect: on, show: on });

/** The Notebook toggle changes visibility only: collection is left as it is (null). */
export const notebookBacksightsChange = (on: boolean): { collect: boolean | null; show: boolean } => ({ collect: null, show: on });
