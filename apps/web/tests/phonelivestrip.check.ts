// The phone live shot strip's decisions (lib/phone-livestrip.ts). Run: npm run test:phonelivestrip
import { dotState, pressKind, splitUnit, stripModel, type StripInput } from '../src/lib/phone-livestrip.ts';

let failures = 0;
function check(what: string, ok: boolean) {
  if (!ok) failures += 1;
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
}
const base: StripInput = { status: 'connected', collapsed: false, held: 0, aimsFirst: false, feature: false, canTakeShot: true, awaiting: false, hasTo: true, burst: 0, aiming: '', message: '', warnings: 0 };
const m = (o: Partial<StripInput>) => stripModel({ ...base, ...o });

check('dots', dotState('connected') === 'green' && dotState('connecting') === 'amber' && dotState('lost') === 'amber' && dotState('idle') === 'grey' && dotState('error') === 'grey');
check('connected shows numbers and a shot button', m({}).view === 'numbers' && m({}).shot?.enabled === true && !m({}).dotOpensSettings);
check('idle collapses to dot + Connect', m({ status: 'idle' }).view === 'connect' && m({ status: 'idle' }).connectLabel === 'Connect' && m({ status: 'idle' }).shot === null && m({ status: 'idle' }).dotOpensSettings);
check('connecting offers Stop', m({ status: 'connecting' }).connectLabel === 'Stop' && m({ status: 'lost' }).connectLabel === 'Stop');
check('collapsed pref wins while connected and while idle', m({ collapsed: true }).view === 'collapsed' && m({ collapsed: true, status: 'idle' }).view === 'collapsed');
check('held readings replace the numbers, even folded', m({ held: 2 }).view === 'held' && m({ held: 1, collapsed: true }).view === 'held' && m({ held: 1 }).shot === null);
check('cannot take shot disables the button', m({ canTakeShot: false }).shot?.enabled === false);
check('feature shoots the feature', m({ feature: true }).shot?.kind === 'feature' && m({ feature: true }).shot?.press === null);
check('X1 single press, enabled', m({ aimsFirst: true }).shot?.press === 'single' && m({ aimsFirst: true }).shot?.enabled === true);
check('X1 awaiting uses leg', pressKind(true) === 'leg' && m({ aimsFirst: true, awaiting: true }).shot?.press === 'leg' && m({ aimsFirst: true, awaiting: true }).shot?.enabled === true);
check('X1 leg with no To and not awaiting is never chosen', m({ aimsFirst: true, hasTo: false }).shot?.press === 'single');
check('X1 burst disables', m({ aimsFirst: true, burst: 2 }).shot?.enabled === false);
check('X1 armed when its kind is aiming', m({ aimsFirst: true, aiming: 'single' }).shot?.armed === true && m({ aimsFirst: true, aiming: 'leg' }).shot?.armed === false);
check('problem marker', m({}).problem === '' && m({ message: 'x' }).problem === 'problem' && m({ warnings: 1 }).problem === 'caution' && m({ message: 'x', warnings: 1 }).problem === 'problem' && m({ message: '  ' }).problem === '');
check('splitUnit', splitUnit('12.4 m').value === '12.4' && splitUnit('12.4 m').unit === 'm' && splitUnit('7').unit === '');

if (failures) {
  console.log(`\n${failures} failed`);
  process.exit(1);
}
console.log('\nall passed');
