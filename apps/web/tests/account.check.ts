// Accounts in the web app: the offline pass and (Task 6) the API client against a fake server.
import { generateKeyPairSync } from 'node:crypto';
import { signPass, publicX } from '../../server/src/pass.ts';
import { verifyPass } from '../src/lib/pass.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };

const { privateKey } = generateKeyPairSync('ed25519');
const other = generateKeyPairSync('ed25519').privateKey;
const x = publicX(privateKey);
const now = 1_800_000_000_000;
const pass = signPass(privateKey, { memberId: 'm1', email: 'a@example.com', role: 'admin' }, now);

const good = await verifyPass(pass, x, now + 1000);
ok(good.ok && good.claims.email === 'a@example.com' && good.claims.role === 'admin', 'a fresh pass verifies');
ok((await verifyPass(pass, publicX(other), now)).ok === false, 'another key is refused');
const flipped = pass.slice(0, 10) + (pass[10] === 'A' ? 'B' : 'A') + pass.slice(11);
ok((await verifyPass(flipped, x, now)).ok === false, 'a changed pass is refused');
const late = await verifyPass(pass, x, now + 31 * 86_400_000);
ok(!late.ok && late.why === 'expired', 'an old pass is expired');
ok(!(await verifyPass('junk', x, now)).ok, 'junk is refused');
ok(!(await verifyPass(pass, '', now)).ok, 'no key in the build is refused');

console.log(bad ? `account: ${bad} failed` : 'account: all ok');
process.exit(bad ? 1 : 0);
