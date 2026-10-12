// Synthetic test caves (no real entrance, no real coordinates) that ship with the Rust tests.
// They stand in until the file importers exist.
import pitfall from '../../../../crates/cave-survey/tests/golden/PitfallCave.json?raw';
import fingerprint from '../../../../crates/cave-survey/tests/golden/FingerprintCave.json?raw';
import plumbline from '../../../../crates/cave-survey/tests/golden/PlumblinePit.json?raw';
import stairstep from '../../../../crates/cave-survey/tests/golden/StairstepCave.json?raw';

export const DEMOS: { id: string; title: string; json: string }[] = [
  { id: 'pitfall', title: 'Pitfall Cave (4 trips, 5 loops)', json: pitfall },
  { id: 'fingerprint', title: 'Fingerprint Cave', json: fingerprint },
  { id: 'plumbline', title: 'Plumbline Pit (vertical)', json: plumbline },
  { id: 'stairstep', title: 'Stairstep Cave', json: stairstep },
];
