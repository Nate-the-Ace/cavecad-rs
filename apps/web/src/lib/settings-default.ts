// The settings sections the app ships with. Phone (the Settings tab): Connection, Survey, Notebook, Sketch, Cave, Display, Account and
// About. Dock (the user window's Options tab on desktop, web and iPad): Display, Symbols and, while instruments are on, Survey
// (the Map's live shot strip): dock-sized components of their own. A module
// adds its own through SETTINGS_SECTIONS (lib/settings-sections.ts); neither place names a section itself.
import { SETTINGS_SECTIONS } from './settings-sections.ts';
import SettingsConnection from '../components/SettingsConnection.svelte';
import SettingsSurvey from '../components/SettingsSurvey.svelte';
import SettingsNotebook from '../components/SettingsNotebook.svelte';
import SettingsSketch from '../components/SettingsSketch.svelte';
import SettingsCave from '../components/SettingsCave.svelte';
import SettingsDisplay from '../components/SettingsDisplay.svelte';
import SettingsAccount from '../components/SettingsAccount.svelte';
import SettingsAbout from '../components/SettingsAbout.svelte';
import DockSettingsDisplay from '../components/DockSettingsDisplay.svelte';
import DockSettingsSymbols from '../components/DockSettingsSymbols.svelte';
import DockSettingsSurvey from '../components/DockSettingsSurvey.svelte';
import { showInstruments } from './features';

let done = false;
/** Registers the built-in sections once. */
export function registerDefaultSettings(): void {
  if (done) return;
  done = true;
  SETTINGS_SECTIONS.register({ id: 'connection', apiVersion: 1, label: 'Connection', order: 10, component: SettingsConnection, layouts: ['phone'] });
  SETTINGS_SECTIONS.register({ id: 'survey', apiVersion: 1, label: 'Survey', order: 20, component: SettingsSurvey, layouts: ['phone'] });
  SETTINGS_SECTIONS.register({ id: 'notebook', apiVersion: 1, label: 'Notebook', order: 30, component: SettingsNotebook, layouts: ['phone'] });
  SETTINGS_SECTIONS.register({ id: 'sketch', apiVersion: 1, label: 'Sketch', order: 35, component: SettingsSketch, layouts: ['phone'] });
  SETTINGS_SECTIONS.register({ id: 'cave', apiVersion: 1, label: 'Cave', order: 38, component: SettingsCave, layouts: ['phone'] });
  SETTINGS_SECTIONS.register({ id: 'display', apiVersion: 1, label: 'Display', order: 40, component: SettingsDisplay, layouts: ['phone'] });
  SETTINGS_SECTIONS.register({ id: 'account', apiVersion: 1, label: 'Account', order: 50, component: SettingsAccount, layouts: ['phone'] });
  SETTINGS_SECTIONS.register({ id: 'dock-display', apiVersion: 1, label: 'Display', order: 40, component: DockSettingsDisplay, layouts: ['dock'] });
  SETTINGS_SECTIONS.register({ id: 'dock-symbols', apiVersion: 1, label: 'Symbols', order: 45, component: DockSettingsSymbols, layouts: ['dock'] });
  if (showInstruments) SETTINGS_SECTIONS.register({ id: 'dock-survey', apiVersion: 1, label: 'Survey', order: 47, component: DockSettingsSurvey, layouts: ['dock'] });
  SETTINGS_SECTIONS.register({ id: 'about', apiVersion: 1, label: 'About', order: 60, component: SettingsAbout, layouts: ['phone'] });
}
