// The phone layout: on a screen the size of a phone (an iPhone either way up), CaveCAD shows one panel at a time with
// a row of tabs to switch between them, instead of the dock of panels and the ribbon. Which tab is showing is kept on
// this device.

/** Narrower than this, or shorter than PHONE_HEIGHT (a phone held sideways), is a phone. */
const PHONE_WIDTH = 600;
const PHONE_HEIGHT = 500;
const QUERY = `(max-width: ${PHONE_WIDTH}px), (max-height: ${PHONE_HEIGHT}px)`;
const TAB_KEY = 'cavecad.phoneTab';

/** The panels the phone layout offers, in tab order (ids as the workspace knows them). */
export const PHONE_TABS = ['survey', 'map', 'section', 'notebook', 'stations', 'profile'] as const;
export type PhoneTab = (typeof PHONE_TABS)[number];

const media = typeof window !== 'undefined' && window.matchMedia ? window.matchMedia(QUERY) : null;

function loadTab(): PhoneTab {
  try {
    const t = localStorage.getItem(TAB_KEY);
    return (PHONE_TABS as readonly string[]).includes(t ?? '') ? (t as PhoneTab) : 'survey';
  } catch {
    return 'survey';
  }
}

export const phone = $state<{ on: boolean; tab: PhoneTab }>({ on: media?.matches ?? false, tab: loadTab() });

media?.addEventListener('change', (e) => (phone.on = e.matches));

/** Shows a panel on the phone. False when the phone layout has no tab for it (the caller does nothing then). */
export function showPhoneTab(id: string): boolean {
  if (!(PHONE_TABS as readonly string[]).includes(id)) return false;
  phone.tab = id as PhoneTab;
  try {
    localStorage.setItem(TAB_KEY, id);
  } catch {
    /* the tab is just not remembered */
  }
  return true;
}
