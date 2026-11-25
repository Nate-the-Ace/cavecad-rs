// The phone Settings tab is a row of sections (Connection, Survey, Display, Account, About). A module adds one by
// registering it here; the tab builds its switcher from `settingsSections()` and never names a section itself.
import type { Component } from 'svelte';
import { Registry, type Plugin } from './plugin.ts';

export interface SettingsSection extends Plugin {
  /** The short label in the switcher row. */
  readonly label: string;
  /** Position in the row, low first. */
  readonly order: number;
  /** The body, shown below the switcher. It must fit one screen: only a data list may scroll, inside a bounded box. */
  readonly component: Component;
}

export const SETTINGS_SECTIONS = new Registry<SettingsSection>('settings', 1);

/** The sections in switcher order. */
export const settingsSections = (): SettingsSection[] => SETTINGS_SECTIONS.list().sort((a, b) => a.order - b.order);
