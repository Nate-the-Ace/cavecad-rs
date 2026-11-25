// The phone Survey screen's words (lib/phone-survey.ts): the status pill, and the at-most-two problem lines.
// Run: npm run test:phonesurvey
import { isLinked, linkLabel, problemLines, sweepName, type ProblemInput } from '../src/lib/phone-survey.ts';

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
