// The phone Settings sections the app ships with: Connection, Survey, Display, Account and About. A module adds its
// own through SETTINGS_SECTIONS (lib/settings-sections.ts); the tab never names a section itself.
import { SETTINGS_SECTIONS } from './settings-sections.ts';
import SettingsConnection from '../components/SettingsConnection.svelte';
import SettingsSurvey from '../components/SettingsSurvey.svelte';
import SettingsDisplay from '../components/SettingsDisplay.svelte';
import SettingsAccount from '../components/SettingsAccount.svelte';
import SettingsAbout from '../components/SettingsAbout.svelte';

let done = false;
/** Registers the built-in sections once. */
export function registerDefaultSettings(): void {
  if (done) return;
  done = true;
  SETTINGS_SECTIONS.register({ id: 'connection', apiVersion: 1, label: 'Connection', order: 10, component: SettingsConnection });
  SETTINGS_SECTIONS.register({ id: 'survey', apiVersion: 1, label: 'Survey', order: 20, component: SettingsSurvey });
  SETTINGS_SECTIONS.register({ id: 'display', apiVersion: 1, label: 'Display', order: 30, component: SettingsDisplay });
  SETTINGS_SECTIONS.register({ id: 'account', apiVersion: 1, label: 'Account', order: 40, component: SettingsAccount });
  SETTINGS_SECTIONS.register({ id: 'about', apiVersion: 1, label: 'About', order: 50, component: SettingsAbout });
}
