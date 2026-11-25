// The phone layout: on a screen the size of a phone (an iPhone either way up), CaveCAD shows one screen at a time with
// a row of tabs along the top (Caves, Survey, Sketch, Notebook, Settings) instead of the dock of panels and the ribbon.
// Sketch holds the Map, Profile and Cross Section behind a segmented switch; Notebook holds the Notebook and Stations.
// Which tab (and which view inside it) is showing is kept on this device.

/** Narrower than this, or shorter than PHONE_HEIGHT (a phone held sideways), is a phone. */
const PHONE_WIDTH = 600;
const PHONE_HEIGHT = 500;
const QUERY = `(max-width: ${PHONE_WIDTH}px), (max-height: ${PHONE_HEIGHT}px)`;
const TAB_KEY = 'cavecad.phoneTab';

/** The phone's tabs, left to right: Caves far left, Settings far right. */
export const PHONE_TABS = ['caves', 'survey', 'sketch', 'notebook', 'settings'] as const;
export type PhoneTab = (typeof PHONE_TABS)[number];
export type SketchView = 'map' | 'profile' | 'section';
export type NotebookView = 'notebook' | 'stations';

/** Where a workspace panel id (or an old stored tab) lands on the phone. */
const PANEL_TO_TAB: Record<string, { tab: PhoneTab; sketch?: SketchView; notebook?: NotebookView }> = {
  caves: { tab: 'caves' },
  survey: { tab: 'survey' },
  capture: { tab: 'survey' },
  sketch: { tab: 'sketch' },
  map: { tab: 'sketch', sketch: 'map' },
  profile: { tab: 'sketch', sketch: 'profile' },
  section: { tab: 'sketch', sketch: 'section' },
  notebook: { tab: 'notebook', notebook: 'notebook' },
  stations: { tab: 'notebook', notebook: 'stations' },
  settings: { tab: 'settings' },
};

/** A stored tab value, made valid: old ids (capture, map, stations, ...) map to the tab that holds them now. */
export function sanitizeTab(stored: string | null | undefined): PhoneTab {
  return PANEL_TO_TAB[stored ?? '']?.tab ?? 'survey';
}

const media = typeof window !== 'undefined' && window.matchMedia ? window.matchMedia(QUERY) : null;

function loadTab(): PhoneTab {
  try {
    return sanitizeTab(localStorage.getItem(TAB_KEY));
  } catch {
    return 'survey';
  }
}

export const phone = $state<{ on: boolean; tab: PhoneTab; sketch: SketchView; notebook: NotebookView; settings: string }>({
  on: media?.matches ?? false,
  tab: loadTab(),
  sketch: 'map',
  notebook: 'notebook',
  /** The Settings tab's section showing ('' for its first). */
  settings: '',
});

media?.addEventListener('change', (e) => (phone.on = e.matches));

/**
 * Shows a tab, or the tab holding a workspace panel (map, profile, section, stations, ...). False when the phone
 * layout has nothing for it (the caller does nothing then). While a gate is up the shell still shows Caves only.
 */
export function showPhoneTab(id: string): boolean {
  const to = PANEL_TO_TAB[id];
  if (!to) return false;
  phone.tab = to.tab;
  if (to.sketch) phone.sketch = to.sketch;
  if (to.notebook) phone.notebook = to.notebook;
  try {
    localStorage.setItem(TAB_KEY, to.tab);
  } catch {
    /* the tab is just not remembered */
  }
  return true;
}

/** Shows the Settings tab at one of its sections (lib/settings-sections.ts), e.g. 'connection'. */
export function showSettings(section: string): void {
  phone.settings = section;
  showPhoneTab('settings');
}
