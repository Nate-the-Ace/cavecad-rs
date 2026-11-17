// What changed in CaveCAD, newest first, in plain words: shown once after an update ("What's new"), and again from the
// build label in the status bar. Add an entry at the top for each release worth telling people about; its `id` must
// be new (a date and a word will do), as the splash shows every entry newer than the last one this device has seen.

export interface Change {
  /** Unique, never reused: the splash remembers the newest one seen. */
  id: string;
  /** When it went out, as people read a date. */
  date: string;
  title: string;
  items: string[];
}

export const CHANGES: Change[] = [
  {
    id: '2026-11-17-in-cave',
    date: '17 November 2026',
    title: 'Surveying in the cave with the Cavway X1',
    items: [
      'In-Cave Survey layout: the map and Device Capture side by side (Survey tab, Layout). On a phone, one panel at a time with a single row of tabs.',
      'CaveCAD for iPhone and iPad, connecting straight to the X1 over Bluetooth.',
      'Two shot buttons: Single shot (a splay) and Station to station (three shots in a row, the leg). The first press turns the laser on to aim, the second shoots. Each button says where the shot goes.',
      'Sweeps with live checks: an LRUD ring across the passage or a room all the way round, shown as percent covered. Manual mode asks for the four walls instead.',
      'Shots still in the X1 from before it connected are no longer written at the station you happen to stand at: a card shows them, to add here or discard.',
      'The X1 reconnects by itself after it sleeps. A Disconnect button shows while it is reconnecting.',
      'X1 marks: shots you mark Feature, Ridge or Generic on the X1 (M while the laser is on) are coloured by their mark, and can place a symbol or draw a line through their hits (Device control, X1 marks).',
      'Guided calibration follows the X1 and says where to point next. The X1 quick reference is in the app and on a printable card.',
      'Accuracy check (Strict, Normal, Relaxed), with the X1\'s complaints said in plain words.',
      'Entrance from your location: Use my location finds the likely entrance, with or without signal.',
      'Every kind of thing has its own colour (legs, splays, sweeps, walls, water, formations, rigging and more), and symbols take the colour of their family.',
      'Sheets print in Black or Colour.',
    ],
  },
];

const KEY = 'cavecad.changelog.seen';

/** The entries this device has not been shown yet, newest first (a device that has never seen one gets the latest). */
export function unseenChanges(): Change[] {
  let seen: string | null = null;
  try {
    seen = localStorage.getItem(KEY);
  } catch {
    return [];
  }
  if (seen === null) return CHANGES.slice(0, 1); // never shown: the latest release, once
  const i = CHANGES.findIndex((c) => c.id === seen);
  return i < 0 ? CHANGES.slice(0, 1) : CHANGES.slice(0, i);
}

/** Remembers that the newest entry has been shown. */
export function markChangesSeen() {
  try {
    if (CHANGES[0]) localStorage.setItem(KEY, CHANGES[0].id);
  } catch {
    // shown again next time
  }
}
