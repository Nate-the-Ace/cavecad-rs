// The phone Settings sections the app ships with. Connection, Survey, Display and About are placeholders until
// their own step fills them (replace the component here); Account is real.
import { SETTINGS_SECTIONS } from './settings-sections.ts';
import SettingsPlaceholder from '../components/SettingsPlaceholder.svelte';
import SettingsAccount from '../components/SettingsAccount.svelte';

let done = false;
/** Registers the built-in sections once. */
export function registerDefaultSettings(): void {
  if (done) return;
  done = true;
  SETTINGS_SECTIONS.register({ id: 'connection', apiVersion: 1, label: 'Connection', order: 10, component: SettingsPlaceholder });
  SETTINGS_SECTIONS.register({ id: 'survey', apiVersion: 1, label: 'Survey', order: 20, component: SettingsPlaceholder });
  SETTINGS_SECTIONS.register({ id: 'display', apiVersion: 1, label: 'Display', order: 30, component: SettingsPlaceholder });
  SETTINGS_SECTIONS.register({ id: 'account', apiVersion: 1, label: 'Account', order: 40, component: SettingsAccount });
  SETTINGS_SECTIONS.register({ id: 'about', apiVersion: 1, label: 'About', order: 50, component: SettingsPlaceholder });
}
