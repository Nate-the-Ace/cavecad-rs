// What the ribbon shows when something in the survey is picked: one entry per kind of pickable thing.
// To make another thing (a drawn feature, a trip, ...) pickable: add its kind to `PickKind` in
// `cave.svelte.ts`, make a component that draws the `part`s listed here (and edits through the engine as
// one undoable step in its panel's own history), and register it below. The ribbon builds the tab from it.

import type { Component } from 'svelte';
import type { PickKind } from './cave.svelte';
import StationTab from '../components/StationTab.svelte';
import ShotTab from '../components/ShotTab.svelte';
import HaloTab from '../components/HaloTab.svelte';

export interface Inspector {
  /** The tab's name. */
  title: string;
  /** The tab's groups, each drawn by `component` given its `part` (and the picked `kind`). */
  groups: { name: string; part: string }[];
  component: Component<any>;
}

const shot = (title: string): Inspector => ({
  title,
  groups: [
    { name: title, part: 'info' },
    { name: 'Measured', part: 'reading' },
    { name: 'Note and flags', part: 'note' },
    { name: 'Mark', part: 'mark' },
  ],
  component: ShotTab,
});

export const INSPECTORS: Record<Exclude<PickKind, ''>, Inspector> = {
  station: {
    title: 'Station',
    groups: [
      { name: 'Station', part: 'info' },
      { name: 'Passage (LRUD)', part: 'walls' },
      { name: 'Notes', part: 'notes' },
      { name: 'Mark', part: 'mark' },
    ],
    component: StationTab,
  },
  halo: { title: 'Rock halo', groups: [{ name: 'Rock halo', part: 'main' }], component: HaloTab },
  leg: shot('Survey leg'),
  splay: shot('Splay'),
};
