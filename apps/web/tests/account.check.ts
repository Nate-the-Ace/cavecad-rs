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

// ---- the API client against the real server code, in this process ----
import { createServer } from 'node:http';
import { openDb } from '../../server/src/db.ts';
import { handleApi } from '../../server/src/api.ts';
import { accountApi, AccountError } from '../src/lib/account-api.ts';

const db = openDb(':memory:');
let user: string | null = 'nathan@example.com';
const server = createServer((req, res) => void handleApi({
  db, adminEmail: 'nathan@example.com', passKey: privateKey, publicUrl: 'https://cave.test',
  mailer: { url: '', key: '' },
  whoIs: async () => (user ? { email: user, name: user.split('@')[0], provider: null, subject: '' } : null),
}, req, res));
await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
const api = accountApi({ base, fetch });

const m = await api.me();
ok(m.kind === 'member' && m.member.role === 'admin' && m.firstVisit, 'first admin arrives as a member on first visit');
const made = await api.invite('rick@example.com', 'member');
ok(made.emailed === false && made.link.startsWith('https://cave.test/?signup='), 'invite made, not emailed with no mailer');
user = 'rick@example.com';
const rick = await api.me(made.link.split('signup=')[1]);
ok(rick.kind === 'member' && rick.member.role === 'member', 'invited person gets in with the link');
let code = '';
try { await api.members(); } catch (e) { code = e instanceof AccountError ? e.code : 'other'; }
ok(code === 'not-admin', 'a member cannot list members');
user = 'stranger@example.com';
const s = await api.me();
ok(s.kind === 'not-invited' && s.reason === 'none', 'a stranger is not invited');
user = null;
ok((await api.me()).kind === 'signed-out', 'no identity is signed out');
server.close();
ok((await accountApi({ base: 'http://127.0.0.1:1', fetch }).me()).kind === 'offline', 'no server is offline');
const redirected = accountApi({ base, fetch: (async () => ({ type: 'opaqueredirect', status: 0 })) as unknown as typeof fetch });
ok((await redirected.me()).kind === 'signed-out', 'a redirect to the sign-in page is signed out');
let sent: Headers | undefined;
await accountApi({ base, fetch: (async (_u: string, init: RequestInit) => { sent = new Headers(init.headers); throw new Error('x'); }) as unknown as typeof fetch, bearer: () => 'T' }).me('S');
ok(sent?.get('authorization') === 'Bearer T' && sent?.get('x-cave-signup') === 'S', 'bearer and sign-up headers are sent');

// ---- desktop sign-in helpers ----
import { authorizeUrl, challengeFor, pkcePair, readAuthLink } from '../src/lib/desktop-signin.ts';
ok((await challengeFor('dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk')) === 'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM', 'S256 challenge matches RFC 7636');
const pair = await pkcePair();
ok(pair.verifier.length >= 43 && pair.challenge === (await challengeFor(pair.verifier)), 'a fresh PKCE pair');
const az = new URL(authorizeUrl('https://auth.test/authorize', 'cavecad-desktop', pair.challenge, 'ST'));
ok(az.searchParams.get('response_type') === 'code' && az.searchParams.get('redirect_uri') === 'cavecad://auth' && az.searchParams.get('code_challenge_method') === 'S256' && az.searchParams.get('state') === 'ST' && az.searchParams.get('scope') === 'openid email profile', 'authorize URL');
ok(readAuthLink('cavecad://auth?code=C1&state=ST', 'ST') === 'C1', 'the returning link gives the code');
ok(readAuthLink('cavecad://auth?code=C1&state=XX', 'ST') === null, 'a wrong state is refused');
ok(readAuthLink('cavecad://join?live=abc', 'ST') === null, 'a join link is not a sign-in');

console.log(bad ? `account: ${bad} failed` : 'account: all ok');
process.exit(bad ? 1 : 0);
