// The Survey tab of the ribbon, as groups: the instrument (one Connect for every supported instrument, the simulator, disconnect), the shot settings,
// the simulator's own switches, and the in-cave layout. Defined once so the app's ribbon and the survey harness
// show the same tab. The Survey panel keeps what belongs with the data (the reading, the stations, the passage
// dimensions, the log); the settings are here, as the app keeps a running command's settings in the ribbon.

import { Bluetooth, FlaskConical, Play, Shuffle, Tent, Unplug } from '@lucide/svelte';
import type { Group } from './Ribbon.svelte';
import SimulatorOptions from './SimulatorOptions.svelte';
import SurveyOptions from './SurveyOptions.svelte';
import * as live from '../lib/survey.svelte';
import { survey } from '../lib/survey.svelte';

/** True while a link of this kind is up or coming up. */
const using = (kind: live.LinkKind) => survey.kind === kind && (survey.status === 'connected' || survey.status === 'connecting' || survey.status === 'lost');

/** The Survey tab's groups. `inCave` arranges the workspace for surveying (the app passes its own; a host without one leaves it out). */
export function surveyGroups(options: { inCave?: () => void } = {}): Group[] {
  const groups: Group[] = [
    {
      name: 'Instrument',
      items: [
        { label: 'Connect', icon: Bluetooth, run: () => void live.connect('auto'), pressed: () => using('auto') },
        { label: 'Simulator', icon: FlaskConical, run: () => void live.connect('simulator'), pressed: () => using('simulator') },
        { label: 'Disconnect', icon: Unplug, run: live.disconnect },
      ],
    },
    { name: 'Shots', items: [], custom: SurveyOptions },
    {
      name: 'Simulator',
      custom: SimulatorOptions,
      items: [
        { label: 'Run on its own', icon: Play, run: live.toggleRun, pressed: () => survey.running },
        { label: 'LRUD shots', icon: Shuffle, run: () => (survey.randomPassage = !survey.randomPassage), pressed: () => survey.randomPassage },
      ],
    },
  ];
  if (options.inCave) groups.push({ name: 'Layout', items: [{ label: 'In cave', icon: Tent, run: options.inCave }] });
  return groups;
}
