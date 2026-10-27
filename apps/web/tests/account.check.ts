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
import { accountApi, AccountError, arrivalPhase } from '../src/lib/account-api.ts';

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
ok(m.kind === 'member' && m.noticeSeen === false && arrivalPhase(m) === 'welcome', 'a new member sees the welcome page first');
await api.updateMe({ welcomed: true });
const m2 = await api.me();
ok(m2.kind === 'member' && m2.noticeSeen === true && arrivalPhase(m2) === 'ready', 'the welcome page marks the notice too');
// A member welcomed before the notice existed: shown it once, then OK marks it.
db.prepare('UPDATE members SET notice_version = 0').run();
const m3 = await api.me();
ok(m3.kind === 'member' && !m3.firstVisit && arrivalPhase(m3) === 'notice', 'an already-welcomed member who has not seen the notice gets it');
await api.updateMe({ noticeSeen: true });
const m4 = await api.me();
ok(m4.kind === 'member' && arrivalPhase(m4) === 'ready', 'after OK the notice is not shown again');
ok(arrivalPhase({ firstVisit: false }) === 'ready', 'a server that does not report the notice shows none');
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
import { authorizeUrl, challengeFor, keep, keptSignIn, pkcePair, readAuthLink } from '../src/lib/desktop-signin.ts';
ok((await challengeFor('dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk')) === 'E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM', 'S256 challenge matches RFC 7636');
const pair = await pkcePair();
ok(pair.verifier.length >= 43 && pair.challenge === (await challengeFor(pair.verifier)), 'a fresh PKCE pair');
const az = new URL(authorizeUrl('https://auth.test/authorize', 'cavecad-desktop', pair.challenge, 'ST'));
ok(az.searchParams.get('response_type') === 'code' && az.searchParams.get('redirect_uri') === 'cavecad://auth' && az.searchParams.get('code_challenge_method') === 'S256' && az.searchParams.get('state') === 'ST' && az.searchParams.get('scope') === 'openid email profile', 'authorize URL');
const link = (l: string, st: string | null) => JSON.stringify(readAuthLink(l, st));
ok(link('cavecad://auth?code=C1&state=ST', 'ST') === '{"kind":"code","code":"C1"}', 'the returning link gives the code');
ok(link('cavecad://auth?code=C1&state=XX', 'ST') === '{"kind":"failed"}', 'a wrong state did not finish');
ok(link('cavecad://auth?error=access_denied&state=ST', 'ST') === '{"kind":"failed"}', 'a returned error did not finish');
ok(link('cavecad://auth?code=C1&state=ST', null) === '{"kind":"failed"}', 'a link with no sign-in waiting did not finish');
ok(link('cavecad://join?live=abc', 'ST') === 'null', 'a join link is not a sign-in');
ok(link('cavecad://authx?code=C1&state=ST', 'ST') === 'null' && link('https://auth?code=C1&state=ST', 'ST') === 'null' && link('not a url', 'ST') === 'null', 'only cavecad://auth is a sign-in link');
keep({ id_token: 'I1', refresh_token: 'R1', expires_in: 3600 });
keep({ id_token: 'I2', refresh_token: null, expires_in: 3600 });
ok(keptSignIn()?.idToken === 'I2' && keptSignIn()?.refresh === 'R1', 'a refresh without a new refresh token keeps the old one');
keep({ id_token: 'I3', refresh_token: 'R2', expires_in: 3600 });
ok(keptSignIn()?.refresh === 'R2', 'a new refresh token replaces the old one');

// ---- the API client: credentials, the desktop retry, and answers that are not JSON ----
const json = (status: number, body: unknown) => new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } });
const memberBody = { status: 'member', member: { id: 'm1', email: 'a@example.com', displayName: 'A', colour: '#000', role: 'member' }, drives: [], firstVisit: false, signups: 'invite', pass: 'p.q' };
{
  const seen: RequestInit[] = [];
  const f = (async (_u: string, init: RequestInit) => (seen.push(init), json(200, memberBody))) as unknown as typeof fetch;
  const a = accountApi({ base: '', fetch: f });
  await a.me();
  await a.updateMe({ colour: '#000' });
  ok(seen.length === 2 && seen.every((i) => i.credentials === 'same-origin'), 'every request sends same-origin credentials only');
}
{
  let token = 'OLD';
  let refreshed = 0;
  const auths: string[] = [];
  const f = (async (_u: string, init: RequestInit) => {
    const t = new Headers(init.headers).get('authorization') ?? '';
    auths.push(t);
    return t === 'Bearer NEW' ? json(200, memberBody) : json(401, { error: 'signed-out' });
  }) as unknown as typeof fetch;
  let signedOut = 0;
  const a = accountApi({ base: '', fetch: f, bearer: () => token, onUnauthorized: async () => { refreshed++; token = 'NEW'; return true; }, onSignedOut: () => signedOut++ });
  ok((await a.me()).kind === 'member' && refreshed === 1 && auths.join() === 'Bearer OLD,Bearer NEW', 'me: a 401 refreshes the desktop token and asks once more');
  token = 'OLD';
  await a.updateMe({ colour: '#000' });
  ok(refreshed === 2 && signedOut === 0, 'a member call is retried after a refresh too');
  const never = accountApi({ base: '', fetch: f, bearer: () => 'OLD', onUnauthorized: async () => (refreshed++, false), onSignedOut: () => signedOut++ });
  let c2 = '';
  try { await never.updateMe({ colour: '#000' }); } catch (e) { c2 = e instanceof AccountError ? e.code : 'other'; }
  ok(c2 === 'signed-out' && signedOut === 1, 'when no fresh token comes, a member call says the sign-in expired');
  const before = auths.length;
  ok((await never.me()).kind === 'signed-out' && auths.length === before + 1, 'no fresh token: no second try');
}
{
  const html = (async () => new Response('<html>portal</html>', { status: 200, headers: { 'Content-Type': 'text/html' } })) as unknown as typeof fetch;
  ok((await accountApi({ base: '', fetch: html }).me()).kind === 'offline', 'a 200 that is not JSON is offline');
  let c3 = '';
  try { await accountApi({ base: '', fetch: html }).updateMe({}); } catch (e) { c3 = e instanceof AccountError ? e.code : 'other'; }
  ok(c3 === 'error', 'a member call answered with a page is an error');
}

// ---- the offline rules ----
import { offlinePhase, passMember, singleFlight } from '../src/lib/account-api.ts';
ok(offlinePhase('ok') === 'offline' && offlinePhase('expired') === 'offline-expired', 'a good pass opens offline, an old one read-only');
ok(offlinePhase('no-key') === 'signed-out' && offlinePhase('malformed') === 'signed-out' && offlinePhase('signature') === 'signed-out', 'a pass that cannot be trusted signs out');
const stored = { ...memberBody.member, role: 'admin' as const, email: 'changed@example.com', provider: null, invitedBy: null, createdAt: 0, lastSeenAt: null, welcomedAt: null };
const shown = passMember(stored, { memberId: 'm1', email: 'a@example.com', role: 'member' });
ok(shown.role === 'member' && shown.email === 'a@example.com' && shown.displayName === 'A', 'offline, who and what role come from the signed pass');
{
  let runs = 0;
  let release = () => {};
  const job = singleFlight(() => new Promise<number>((r) => { runs++; release = () => r(runs); }));
  const p1 = job();
  const p2 = job();
  release();
  ok(p1 === p2 && (await p1) === 1 && runs === 1, 'a second start while one runs gets the same run');
  const p3 = job();
  release();
  ok(p3 !== p1 && (await p3) === 2, 'and a later start runs again');
}

// ---- read-only: the account's lock ----
import { readOnly, setAccountLock } from '../src/lib/readonly.ts';
setAccountLock(true, 'old pass');
ok(readOnly.on && readOnly.why === 'old pass', 'an old pass locks editing');
setAccountLock(false);
ok(!readOnly.on, 'coming back online: editing again');

console.log(bad ? `account: ${bad} failed` : 'account: all ok');
process.exit(bad ? 1 : 0);
