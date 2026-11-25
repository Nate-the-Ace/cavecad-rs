// The phone Survey screen's words (lib/phone-survey.ts): the status pill, and the at-most-two problem lines.
// Run: npm run test:phonesurvey
import { atStationOnly, isLinked, linkLabel, parseShotType, parseSweepKind, problemLines, shotEnabled, shotLabel, shotPress, sweepName, type ProblemInput, type ShotPressInput } from '../src/lib/phone-survey.ts';

let failures = 0;
function check(what: string, ok: boolean) {
  if (!ok) failures += 1;
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
}

check('connected shows the device name', linkLabel('connected', 'Cavway X1 1234') === 'Cavway X1 1234');
check('connected without a name', linkLabel('connected', '') === 'Connected');
check('idle says no instrument', linkLabel('idle', 'old name') === 'No instrument');
check('lost is reconnecting', linkLabel('lost', 'X1') === 'Reconnecting…');
check('error', linkLabel('error', '') === 'Could not connect');
check('connecting, connected and lost are linked', isLinked('connecting') && isLinked('connected') && isLinked('lost'));
check('idle, error and unsupported are not', !isLinked('idle') && !isLinked('error') && !isLinked('unsupported'));

// the default shot type and sweep kind (Settings > Survey)
check('shot type defaults to single', parseShotType(null) === 'single' && parseShotType('') === 'single' && parseShotType('junk') === 'single');
check('shot type leg is kept', parseShotType('leg') === 'leg' && parseShotType('single') === 'single');
check('sweep kind defaults to lrud', parseSweepKind(null) === 'lrud' && parseSweepKind('x') === 'lrud' && parseSweepKind('room') === 'room');
const sp = (o: Partial<ShotPressInput>) => shotPress({ preferred: 'single', awaiting: false, atOnly: false, hasTo: true, ...o });
check('single default presses single', sp({}) === 'single');
check('leg default presses leg', sp({ preferred: 'leg' }) === 'leg');
check('a backsight is always the leg press', sp({ awaiting: true }) === 'leg' && sp({ preferred: 'leg', awaiting: true }) === 'leg');
check('at one station only it is a single', sp({ preferred: 'leg', atOnly: true }) === 'single');
check('leg default with no To falls back to single', sp({ preferred: 'leg', hasTo: false }) === 'single');
check('at station only: lrud mode or a sweep, never while awaiting', atStationOnly({ awaiting: false, mode: 'lrud', sweeping: false }) && atStationOnly({ awaiting: false, mode: 'leg', sweeping: true }) && !atStationOnly({ awaiting: true, mode: 'lrud', sweeping: true }) && !atStationOnly({ awaiting: false, mode: 'leg', sweeping: false }));
const en = { canTakeShot: true, burst: 0, awaiting: false, hasTo: true };
check('enabled: single and leg when ready', shotEnabled('single', en) && shotEnabled('leg', en));
check('disabled while a burst fires or the instrument cannot shoot', !shotEnabled('leg', { ...en, burst: 2 }) && !shotEnabled('single', { ...en, canTakeShot: false }));
check('a leg needs a To or a backsight', !shotEnabled('leg', { ...en, hasTo: false }) && shotEnabled('leg', { ...en, hasTo: false, awaiting: true }));
const lab = { aiming: '' as const, burst: 0, legShots: 3, singleWhere: 'Splay from A1', legWhere: 'A1 → A2' };
check('label says where', shotLabel('single', lab) === 'Splay from A1' && shotLabel('leg', lab) === 'A1 → A2');
check('label once aimed', shotLabel('single', { ...lab, aiming: 'single' }) === 'Shoot' && shotLabel('leg', { ...lab, aiming: 'leg' }) === 'Shoot leg');
check('aimed for the other kind does not say Shoot', shotLabel('leg', { ...lab, aiming: 'single' }) === 'A1 → A2');
check('label while the leg fires', shotLabel('leg', { ...lab, burst: 3 }) === 'Leg 1 of 3…' && shotLabel('leg', { ...lab, burst: 1 }) === 'Leg 3 of 3…');

check('sweep names', sweepName('lrud', true) === 'LRUD' && sweepName('room', true) === 'Room outline' && sweepName('lrud', false) === 'LRUD ring' && sweepName('room', false) === 'Room sweep');

const calm: ProblemInput = { message: '', detail: '', status: 'connected', warnings: [], noBluetooth: false, sweepDue: '' };
check('nothing wrong: no line', problemLines(calm).length === 0);
check('a problem comes first', problemLines({ ...calm, message: 'Refused', warnings: ['Magnetic 6%'] })[0]?.kind === 'problem');
check('never more than two lines', problemLines({ ...calm, message: 'a', warnings: ['b', 'c', 'd'], sweepDue: 'A3' }).length === 2);
check('the link detail only while failing', problemLines({ ...calm, detail: 'Out of range' }).length === 0 && problemLines({ ...calm, status: 'error', detail: 'Out of range' }).length === 1);
check('no stale detail once idle', problemLines({ ...calm, status: 'idle', detail: 'Cavway X1' }).length === 0);
check('the same words once', problemLines({ ...calm, message: 'x', warnings: ['x'] }).length === 1);
check('blank warnings are skipped', problemLines({ ...calm, warnings: ['  '] }).length === 0);
check('sweep due', problemLines({ ...calm, sweepDue: 'A3' })[0]?.text === 'Sweep A3 before moving on.');
check('no Bluetooth only when not linked', problemLines({ ...calm, status: 'idle', noBluetooth: true }).length === 1 && problemLines({ ...calm, noBluetooth: true }).length === 0);

console.log(failures ? `${failures} failed` : 'phonesurvey: all passed');
process.exit(failures ? 1 : 0);
