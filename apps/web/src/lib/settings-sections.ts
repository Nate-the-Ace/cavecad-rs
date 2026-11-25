// The phone Settings tab is a row of sections (Connection, Survey, Notebook, Sketch, Cave, Display, Account, About); the dock's user
// window (Options tab) lists the sections flagged for the dock (Display, Symbols). A module adds one by registering it here, saying which
// layouts show it; each place builds its list from `settingsSectionsFor(layout)` and never names a section itself.
import type { Component } from 'svelte';
import { Registry, type Plugin } from './plugin.ts';

export interface SettingsSection extends Plugin {
  /** The short label in the switcher row. */
  readonly label: string;
  /** Position in the row, low first. */
  readonly order: number;
  /** The body, shown below the switcher. It must fit one screen: only a data list may scroll, inside a bounded box. */
  readonly component: Component;
  /** Where it shows: 'phone' (the Settings tab) and/or 'dock' (the user window's Options tab, on desktop, web and iPad). */
  readonly layouts: readonly SettingsLayout[];
}

/** The phone's Settings tab, or the dock layout's user window. */
export type SettingsLayout = 'phone' | 'dock';

export const SETTINGS_SECTIONS = new Registry<SettingsSection>('settings', 1);

/** Every section in switcher order, whatever its layout. */
export const settingsSections = (): SettingsSection[] => SETTINGS_SECTIONS.list().sort((a, b) => a.order - b.order);

/** The sections one layout shows, in order. */
export const settingsSectionsFor = (layout: SettingsLayout): SettingsSection[] => settingsSections().filter((s) => s.layouts.includes(layout));
