// The phone layout's gates (lib/gates.ts): sign-in first, then a cave must be open.
// Run: npm run test:gates
import { GATES, currentGate, signedInPhase, type GateCheck } from '../src/lib/gates.ts';

let failures = 0;
function check(what: string, ok: boolean) {
  if (!ok) failures += 1;
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}`);
}

check('signed out and no cave: sign-in comes first', currentGate({ signedIn: false, caveOpen: false })?.reason === 'login');
check('signed out with a cave still open: sign-in', currentGate({ signedIn: false, caveOpen: true })?.reason === 'login');
check('signed in, no cave: the cave gate', currentGate({ signedIn: true, caveOpen: false })?.reason === 'cave');
check('the cave gate says what to do', currentGate({ signedIn: true, caveOpen: false })?.message === 'Pick or start a cave to continue');
check('signed in with a cave: open', currentGate({ signedIn: true, caveOpen: true }) === null);
check('two gates, in order', GATES.length === 2);

const extra: GateCheck = (s) => (s.caveOpen ? { reason: 'cave', message: 'extra' } : null);
check('a later gate only speaks once the earlier ones let through', currentGate({ signedIn: true, caveOpen: true }, [...GATES, extra])?.message === 'extra');
check('no checks: open', currentGate({ signedIn: false, caveOpen: false }, []) === null);

for (const p of ['ready', 'offline', 'offline-expired']) check(`${p} counts as signed in`, signedInPhase(p));
for (const p of ['loading', 'welcome', 'notice', 'not-invited', 'removed', 'signed-out']) check(`${p} does not`, !signedInPhase(p));

console.log(failures ? `${failures} failed` : 'gates: all passed');
process.exit(failures ? 1 : 0);
