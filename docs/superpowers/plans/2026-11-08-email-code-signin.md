# Emailed-code sign-in, own sign-in, desktop downloads: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:subagent-driven-development (recommended) or superpowers-extended-cc:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Anyone invited, at any email address, signs in to CaveCAD (web and desktop) with Google or a 6-digit emailed code, and members can download the desktop app from the web app.

**Architecture:** The CaveCAD server (apps/server, Node 26, no dependencies) takes over sign-in from TinyAuth: its own sessions (hashed random tokens in SQLite, cookie for web, bearer for desktop), emailed codes (HMAC-stored, rate-limited, sent through the existing Apps Script mailer), Google OAuth 2 with PKCE (ID token verified against Google's JWKS), and a PKCE desktop hand-off. identity.ts keeps its role as the one place that decides who a request is. The edge drops `forward_auth` for cavecad.com.

**Tech Stack:** Node 26 (`node:sqlite`, `node:crypto`, `node --test`), TypeScript run directly by Node, Svelte 5 web app (checks run with `node --experimental-strip-types`), Google Apps Script mailer, Caddy on the edge VM.

**Spec:** `docs/superpowers/specs/2026-11-08-email-code-signin-design.md`

**User decisions (already made):**
- "Emailed code" for people without Google (2026-11-08).
- "Both from day one": codes work on website and desktop app.
- Downloads for Mac/Windows/Linux inside the web app.
- "Free, with instructions": the desktop app stays unsigned; each download shows first-open steps.
- Google Drive access: shelved, not part of this work.

---

## File map

| File | Job |
|---|---|
| `apps/server/src/db.ts` (modify) | New tables: sessions, email_codes, code_requests, handoffs |
| `apps/server/src/jwt.ts` (create) | Verify an RS256/EdDSA JWT against a JWKS; fetch and cache a JWKS URL |
| `apps/server/src/sessions.ts` (create) | Create/find/revoke sessions; cookie helpers |
| `apps/server/src/email-code.ts` (create) | Request and verify emailed codes, limits |
| `apps/server/src/google.ts` (create) | Google OAuth start + callback |
| `apps/server/src/handoff.ts` (create) | Desktop PKCE hand-off codes |
| `apps/server/src/signin.ts` (create) | `/signin`, `/signin/code`, `/signin/verify`, `/auth/google*`, `/signout` routes + page |
| `apps/server/src/downloads.ts` (create) | `/downloads/<file>` and `/api/downloads` |
| `apps/server/src/identity.ts` (rewrite) | whoIs from bearer session, cookie session, tailnet, dev |
| `apps/server/src/members.ts` (modify) | `mayGetIn`; provider type |
| `apps/server/src/mailer.ts` (modify) | `sendCode` |
| `apps/server/src/api.ts` (modify) | desktop-token route, removal revokes sessions, signed-out → /signin |
| `apps/server/src/main.ts` (modify) | New env, routing |
| `apps/server/src/desktop-token.ts` (delete) | Replaced by handoff.ts |
| `tools/invite-mailer/{Logic,Code,Code.combined}.gs`, `test.mjs` | `code` kind, send from support@cavecad.com |
| `apps/web/src/lib/desktop-signin.ts` (modify) | New hand-off, no refresh |
| `apps/web/src/lib/account.svelte.ts`, `components/account/AccountGate.svelte`, `AccountSection.svelte` | Sign in/out through `/signin`, `/signout` |
| `apps/web/src/lib/downloads.ts` (create), `components/account/DownloadsPanel.svelte` (create) | Download panel |
| `deploy/*`, `.github/workflows/{web,desktop}.yml`, `apps/server/.env.example` | Deployment |

Run server tests with `cd apps/server && npm test`. Run web checks with `cd apps/web && npm test` (and the single new check named in each task).

---

### Task 1: Database tables

**Goal:** The database has the four tables the new sign-in needs.

**Files:**
- Modify: `apps/server/src/db.ts` (append to `SCHEMA`)
- Test: `apps/server/tests/db.test.ts`

**Acceptance Criteria:**
- [ ] `openDb(':memory:')` creates `sessions`, `email_codes`, `code_requests`, `handoffs`
- [ ] Opening an existing database file twice does not fail

**Verify:** `cd apps/server && node --test tests/db.test.ts` → all pass

**Steps:**

- [ ] **Step 1: Failing test** — append to `tests/db.test.ts`:

```ts
test('sign-in tables exist', () => {
  const db = openDb(':memory:');
  const names = (db.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").all() as { name: string }[]).map((r) => r.name);
  for (const t of ['sessions', 'email_codes', 'code_requests', 'handoffs']) assert.ok(names.includes(t), t);
});
```

(Use the file's existing `test`/`assert`/`openDb` imports.)

- [ ] **Step 2:** `node --test tests/db.test.ts` → FAIL `sessions`.

- [ ] **Step 3: Implement** — add inside the `SCHEMA` template string, after the last `CREATE TABLE`:

```sql
CREATE TABLE IF NOT EXISTS sessions (
  token_hash TEXT PRIMARY KEY,
  email TEXT NOT NULL,
  name TEXT NOT NULL DEFAULT '',
  provider TEXT,
  kind TEXT NOT NULL CHECK (kind IN ('web', 'desktop')),
  created_at INTEGER NOT NULL,
  expires_at INTEGER NOT NULL,
  revoked_at INTEGER
);
CREATE INDEX IF NOT EXISTS sessions_email ON sessions(email);
CREATE TABLE IF NOT EXISTS email_codes (
  email TEXT PRIMARY KEY,
  code_hmac TEXT NOT NULL,
  expires_at INTEGER NOT NULL,
  tries_left INTEGER NOT NULL,
  used_at INTEGER
);
CREATE TABLE IF NOT EXISTS code_requests (email TEXT NOT NULL, ip TEXT NOT NULL, at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS code_requests_at ON code_requests(at);
CREATE TABLE IF NOT EXISTS handoffs (
  code_hash TEXT PRIMARY KEY,
  challenge TEXT NOT NULL,
  email TEXT NOT NULL,
  name TEXT NOT NULL DEFAULT '',
  provider TEXT,
  expires_at INTEGER NOT NULL,
  used_at INTEGER
);
```

- [ ] **Step 4:** test passes; `npm test` still all pass.
- [ ] **Step 5: Commit** `git commit -am "server: tables for own sign-in"`

---

### Task 2: jwt.ts (shared JWT check)

**Goal:** One JWT verifier and JWKS cache, used by Google sign-in.

**Files:**
- Create: `apps/server/src/jwt.ts`
- Test: `apps/server/tests/jwt.test.ts`

**Acceptance Criteria:**
- [ ] A token signed by a key in the set, right `iss`/`aud`, unexpired → claims
- [ ] Wrong signature, wrong `iss`, wrong `aud`, expired, `alg: none` → null
- [ ] `jwksKeys` caches an hour and refetches at most once a minute for an unknown `kid`

**Verify:** `node --test tests/jwt.test.ts` → all pass

**Steps:**

- [ ] **Step 1: Test** — `tests/jwt.test.ts`:

```ts
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync, sign } from 'node:crypto';
import { verifyJwt, jwksKeys } from '../src/jwt.ts';

const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
const jwk = { ...(publicKey.export({ format: 'jwk' }) as object), kid: 'k1', alg: 'RS256', use: 'sig' };
const now = 1_800_000_000;
const make = (claims: object, head: object = { alg: 'RS256', kid: 'k1' }) => {
  const h = Buffer.from(JSON.stringify(head)).toString('base64url');
  const c = Buffer.from(JSON.stringify(claims)).toString('base64url');
  return `${h}.${c}.${sign('sha256', Buffer.from(h + '.' + c), privateKey).toString('base64url')}`;
};
const good = { iss: 'https://accounts.google.com', aud: 'client', exp: now + 600, sub: '1', email: 'a@b.com' };
const opts = { keys: async () => [jwk], issuers: ['https://accounts.google.com', 'accounts.google.com'], audience: 'client', now };

test('a good token gives its claims', async () => {
  assert.equal((await verifyJwt(make(good), opts))?.email, 'a@b.com');
});
test('bad tokens give null', async () => {
  assert.equal(await verifyJwt(make({ ...good, iss: 'x' }), opts), null);
  assert.equal(await verifyJwt(make({ ...good, aud: 'other' }), opts), null);
  assert.equal(await verifyJwt(make({ ...good, exp: now - 120 }), opts), null);
  assert.equal(await verifyJwt(make(good, { alg: 'none', kid: 'k1' }), opts), null);
  const t = make(good);
  assert.equal(await verifyJwt(t.slice(0, -4) + 'AAAA', opts), null);
  assert.equal(await verifyJwt('junk', opts), null);
});
test('jwksKeys caches and refetches for an unknown kid at most once a minute', async () => {
  let calls = 0;
  const fake = (async () => { calls++; return { ok: true, json: async () => ({ keys: [jwk] }) }; }) as unknown as typeof fetch;
  const get = jwksKeys('https://example/jwks', fake);
  await get('k1'); await get('k1');
  assert.equal(calls, 1);
  await get('unknown'); await get('unknown');
  assert.equal(calls, 2);
});
```

- [ ] **Step 2:** run → FAIL (no module).

- [ ] **Step 3: Implement** `src/jwt.ts` (logic moved from identity.ts):

```ts
// Checks a signed JWT (RS256 or EdDSA) against a set of public keys, and fetches a key set from a JWKS URL.
// Google's ID tokens are checked here.
import { createPublicKey, verify, type JsonWebKey } from 'node:crypto';

const LEEWAY = 60;
const isObject = (v: unknown): v is Record<string, unknown> => typeof v === 'object' && v !== null && !Array.isArray(v);

function keyFits(k: JsonWebKey, alg: string): boolean {
  if (k.use === 'enc') return false;
  if (alg === 'RS256') return k.kty === 'RSA';
  if (alg === 'EdDSA') return k.kty === 'OKP' && k.crv === 'Ed25519';
  return false;
}
function checkSignature(jwk: JsonWebKey, alg: string, data: Buffer, sig: Buffer): boolean {
  try {
    const key = createPublicKey({ key: jwk, format: 'jwk' });
    if (alg === 'RS256') return verify('sha256', data, key, sig);
    if (alg === 'EdDSA') return verify(null, data, key, sig);
  } catch {
    // an unusable key verifies nothing
  }
  return false;
}

export type JwtCheck = { keys: (kid?: string) => Promise<JsonWebKey[]>; issuers: string[]; audience: string; now: number };

/** The token's claims when its signature, issuer, audience and times check out; else null. */
export async function verifyJwt(token: string, o: JwtCheck): Promise<Record<string, unknown> | null> {
  const parts = token.split('.');
  if (parts.length !== 3) return null;
  let head: Record<string, unknown>, claims: Record<string, unknown>;
  try {
    const h: unknown = JSON.parse(Buffer.from(parts[0], 'base64url').toString());
    const c: unknown = JSON.parse(Buffer.from(parts[1], 'base64url').toString());
    if (!isObject(h) || !isObject(c)) return null;
    head = h;
    claims = c;
  } catch {
    return null;
  }
  const alg = typeof head.alg === 'string' ? head.alg : '';
  if (alg !== 'RS256' && alg !== 'EdDSA') return null;
  const kid = typeof head.kid === 'string' ? head.kid : undefined;
  const candidates = (await o.keys(kid)).filter((k) => keyFits(k, alg) && (kid === undefined || (k as { kid?: string }).kid === kid));
  const data = Buffer.from(parts[0] + '.' + parts[1]);
  const sig = Buffer.from(parts[2], 'base64url');
  if (!candidates.some((k) => checkSignature(k, alg, data, sig))) return null;
  const aud = Array.isArray(claims.aud) ? claims.aud : [claims.aud];
  if (typeof claims.iss !== 'string' || !o.issuers.includes(claims.iss) || !aud.includes(o.audience)) return null;
  if (aud.length > 1 && claims.azp !== o.audience) return null;
  if (typeof claims.exp !== 'number' || claims.exp + LEEWAY <= o.now) return null;
  if (claims.nbf !== undefined && (typeof claims.nbf !== 'number' || claims.nbf > o.now + LEEWAY)) return null;
  return claims;
}

/**
 * Fetches a JWKS. Keys are cached for an hour, a failed fetch for 30 s (no keys meanwhile), and a token
 * whose kid is unknown triggers at most one refetch a minute.
 */
export function jwksKeys(url: string, fetchImpl: typeof fetch = fetch): (kid?: string) => Promise<JsonWebKey[]> {
  let cache: { at: number; keys: JsonWebKey[] } | null = null;
  let failedAt = -Infinity;
  let refetchedAt = -Infinity;
  const load = async (): Promise<JsonWebKey[]> => {
    try {
      const res = await fetchImpl(url, { signal: AbortSignal.timeout(5000) });
      if (!res.ok) throw new Error(`${url} answered ${res.status}`);
      const set = (await res.json()) as { keys?: unknown };
      if (!Array.isArray(set?.keys)) throw new Error('no keys');
      cache = { at: Date.now(), keys: set.keys as JsonWebKey[] };
      return cache.keys;
    } catch (e) {
      failedAt = Date.now();
      console.error('JWKS:', e instanceof Error ? e.message : e);
      return cache && Date.now() - cache.at < 3_600_000 ? cache.keys : [];
    }
  };
  return async (kid) => {
    const t = Date.now();
    if (cache && t - cache.at < 3_600_000) {
      const known = kid === undefined || cache.keys.some((k) => (k as { kid?: string }).kid === kid);
      if (known || t - refetchedAt < 60_000 || t - failedAt < 30_000) return cache.keys;
      refetchedAt = t;
      return load();
    }
    if (t - failedAt < 30_000) return [];
    return load();
  };
}
```

- [ ] **Step 4:** test passes.
- [ ] **Step 5: Commit** `git add src/jwt.ts tests/jwt.test.ts && git commit -m "server: shared JWT verifier"`

---

### Task 3: sessions.ts

**Goal:** Sessions that say who someone is for 30 days, revocable.

**Files:**
- Create: `apps/server/src/sessions.ts`
- Test: `apps/server/tests/sessions.test.ts`

**Acceptance Criteria:**
- [ ] `createSession` returns a 43-char token; only its SHA-256 is in the table
- [ ] `findSession` gives the Identity until expiry; null after expiry, after `revokeSession`, after `revokeAllFor(email)`
- [ ] Cookie helpers: `sessionCookie(t, true)` contains `HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=2592000`; `readCookie` finds a named cookie among several

**Verify:** `node --test tests/sessions.test.ts` → all pass

**Steps:**

- [ ] **Step 1: Test** — `tests/sessions.test.ts`:

```ts
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { openDb } from '../src/db.ts';
import { createSession, findSession, revokeSession, revokeAllFor, sessionCookie, clearSessionCookie, readCookie, SESSION_MS } from '../src/sessions.ts';

const who = { email: 'Rick@Example.com', name: 'Rick', provider: 'email' as const, subject: '' };
const now = 1_800_000_000_000;

test('a session finds its person until it expires', () => {
  const db = openDb(':memory:');
  const t = createSession(db, who, 'web', now);
  assert.equal(t.length, 43);
  const row = db.prepare('SELECT token_hash FROM sessions').get() as { token_hash: string };
  assert.equal(row.token_hash, createHash('sha256').update(t).digest('hex'));
  assert.deepEqual(findSession(db, t, now + 1000), { email: 'rick@example.com', name: 'Rick', provider: 'email', subject: 'session' });
  assert.equal(findSession(db, t, now + SESSION_MS + 1), null);
  assert.equal(findSession(db, 'nope', now), null);
  assert.equal(findSession(db, '', now), null);
});
test('revoking ends sessions', () => {
  const db = openDb(':memory:');
  const a = createSession(db, who, 'web', now);
  const b = createSession(db, who, 'desktop', now);
  revokeSession(db, a, now);
  assert.equal(findSession(db, a, now), null);
  assert.ok(findSession(db, b, now));
  revokeAllFor(db, 'RICK@example.com', now);
  assert.equal(findSession(db, b, now), null);
});
test('cookies', () => {
  assert.match(sessionCookie('tok', true), /^cavecad_session=tok; HttpOnly; Secure; SameSite=Lax; Path=\/; Max-Age=2592000$/);
  assert.ok(!sessionCookie('tok', false).includes('Secure'));
  assert.match(clearSessionCookie(true), /Max-Age=0/);
  assert.equal(readCookie('a=1; cavecad_session=xyz; b=2', 'cavecad_session'), 'xyz');
  assert.equal(readCookie(undefined, 'cavecad_session'), '');
});
```

- [ ] **Step 2:** run → FAIL.

- [ ] **Step 3: Implement** `src/sessions.ts`:

```ts
// CaveCAD's own sessions: a random token the browser keeps in a cookie (web) or the desktop app sends as a
// bearer token. Only the token's SHA-256 is stored. 30 days, as the offline pass.
import { createHash, randomBytes } from 'node:crypto';
import type { Db } from './db.ts';
import type { Identity } from './members.ts';

export const SESSION_MS = 30 * 86_400_000;
export const SESSION_COOKIE = 'cavecad_session';
const hash = (t: string) => createHash('sha256').update(t).digest('hex');
const norm = (e: string) => e.trim().toLowerCase();

/** Starts a session for `who`; gives back the token (never stored). */
export function createSession(db: Db, who: Identity, kind: 'web' | 'desktop', now: number): string {
  const token = randomBytes(32).toString('base64url');
  db.prepare('INSERT INTO sessions (token_hash, email, name, provider, kind, created_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?)')
    .run(hash(token), norm(who.email), who.name.slice(0, 80), who.provider, kind, now, now + SESSION_MS);
  return token;
}

/** Who a live session belongs to, or null. */
export function findSession(db: Db, token: string, now: number): Identity | null {
  if (!token) return null;
  const r = db.prepare('SELECT email, name, provider FROM sessions WHERE token_hash = ? AND revoked_at IS NULL AND expires_at > ?').get(hash(token), now) as
    | { email: string; name: string; provider: Identity['provider'] }
    | undefined;
  return r ? { email: r.email, name: r.name || r.email.split('@')[0], provider: r.provider, subject: 'session' } : null;
}

export function revokeSession(db: Db, token: string, now: number): void {
  if (token) db.prepare('UPDATE sessions SET revoked_at = ? WHERE token_hash = ? AND revoked_at IS NULL').run(now, hash(token));
}

/** Ends every session of this email (a removed member is signed out everywhere). */
export function revokeAllFor(db: Db, email: string, now: number): void {
  db.prepare('UPDATE sessions SET revoked_at = ? WHERE email = ? AND revoked_at IS NULL').run(now, norm(email));
}

export const sessionCookie = (token: string, secure: boolean) =>
  `${SESSION_COOKIE}=${token}; HttpOnly;${secure ? ' Secure;' : ''} SameSite=Lax; Path=/; Max-Age=${SESSION_MS / 1000}`;
export const clearSessionCookie = (secure: boolean) => `${SESSION_COOKIE}=; HttpOnly;${secure ? ' Secure;' : ''} SameSite=Lax; Path=/; Max-Age=0`;

/** One cookie's value from a Cookie header; '' when absent. */
export function readCookie(header: string | undefined, name: string): string {
  for (const part of (header ?? '').split(';')) {
    const i = part.indexOf('=');
    if (i > 0 && part.slice(0, i).trim() === name) return part.slice(i + 1).trim();
  }
  return '';
}
```

- [ ] **Step 4:** In `src/members.ts` line 14 change the Identity type to
  `provider: 'google' | 'microsoft' | 'email' | null;` — then test passes, `npm test` all pass.
- [ ] **Step 5: Commit** `git add -A src tests && git commit -m "server: sessions"`

---

### Task 4: email-code.ts and members.mayGetIn

**Goal:** Request and check emailed codes without leaking who is a member.

**Files:**
- Create: `apps/server/src/email-code.ts`
- Modify: `apps/server/src/members.ts` (add `mayGetIn` after `mayLoadApp`)
- Test: `apps/server/tests/email-code.test.ts`

**Acceptance Criteria:**
- [ ] Eligible email → code sent (6 digits), answer `{ ok: true }`
- [ ] Stranger → nothing sent, answer `{ ok: true }`
- [ ] Malformed email → `{ ok: false, error: 'bad-email' }`
- [ ] Limits: second request within 60 s, sixth in an hour, 21st from one IP in an hour → nothing sent, answer `{ ok: true }`
- [ ] Mail failure for an eligible email → `{ ok: false, error: 'send-failed' }`
- [ ] Verify: right code once → email; second use → `expired`; wrong code → `wrong` with tries left 4..1, then `expired`; after 10 min → `expired`
- [ ] Table holds an HMAC, never the code

**Verify:** `node --test tests/email-code.test.ts` → all pass

**Steps:**

- [ ] **Step 1: Test** — `tests/email-code.test.ts`:

```ts
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { openDb } from '../src/db.ts';
import { requestCode, verifyCode, CODE_MS } from '../src/email-code.ts';

const now = 1_800_000_000_000;
function setup(sendOk = true) {
  const db = openDb(':memory:');
  const sent: { to: string; code: string }[] = [];
  const d = { db, secret: 'test-secret-0123456789abcdef', send: async (to: string, code: string) => (sent.push({ to, code }), sendOk), mayGetIn: (e: string) => e === 'rick@example.com' };
  return { db, d, sent };
}

test('an eligible email gets a code; a stranger gets the same answer and nothing', async () => {
  const { d, sent, db } = setup();
  assert.deepEqual(await requestCode(d, ' Rick@Example.com ', '1.1.1.1', now), { ok: true });
  assert.equal(sent.length, 1);
  assert.match(sent[0].code, /^\d{6}$/);
  assert.equal(sent[0].to, 'rick@example.com');
  const stored = db.prepare('SELECT code_hmac FROM email_codes').get() as { code_hmac: string };
  assert.notEqual(stored.code_hmac, sent[0].code);
  assert.deepEqual(await requestCode(d, 'stranger@example.com', '1.1.1.1', now), { ok: true });
  assert.equal(sent.length, 1);
  assert.deepEqual(await requestCode(d, 'not an email', '1.1.1.1', now), { ok: false, error: 'bad-email' });
});

test('limits', async () => {
  const { d, sent } = setup();
  await requestCode(d, 'rick@example.com', '1.1.1.1', now);
  await requestCode(d, 'rick@example.com', '1.1.1.1', now + 30_000);
  assert.equal(sent.length, 1, 'one a minute');
  for (let i = 1; i <= 5; i++) await requestCode(d, 'rick@example.com', '1.1.1.1', now + i * 61_000);
  assert.equal(sent.length, 5, 'five an hour');
  const { d: d2, sent: s2 } = setup();
  for (let i = 0; i < 21; i++) await requestCode({ ...d2, mayGetIn: () => true }, `p${i}@example.com`, '9.9.9.9', now + i);
  assert.equal(s2.length, 20, 'twenty an hour from one address');
});

test('mail failure is reported', async () => {
  const { d } = setup(false);
  assert.deepEqual(await requestCode(d, 'rick@example.com', '1.1.1.1', now), { ok: false, error: 'send-failed' });
});

test('verifying', async () => {
  const { d, sent } = setup();
  await requestCode(d, 'rick@example.com', '1.1.1.1', now);
  const code = sent[0].code;
  const wrong = code === '000000' ? '111111' : '000000';
  assert.deepEqual(verifyCode(d, 'rick@example.com', wrong, now), { ok: false, error: 'wrong', triesLeft: 4 });
  assert.deepEqual(verifyCode(d, 'RICK@example.com', code, now), { ok: true, email: 'rick@example.com' });
  assert.deepEqual(verifyCode(d, 'rick@example.com', code, now), { ok: false, error: 'expired' }, 'works once');
  assert.deepEqual(verifyCode(d, 'nobody@example.com', code, now), { ok: false, error: 'expired' });
});

test('tries run out, and codes expire', async () => {
  const { d, sent } = setup();
  await requestCode(d, 'rick@example.com', '1.1.1.1', now);
  const code = sent[0].code;
  const wrong = code === '000000' ? '111111' : '000000';
  for (const left of [4, 3, 2, 1]) assert.deepEqual(verifyCode(d, 'rick@example.com', wrong, now), { ok: false, error: 'wrong', triesLeft: left });
  assert.deepEqual(verifyCode(d, 'rick@example.com', wrong, now), { ok: false, error: 'expired' });
  assert.deepEqual(verifyCode(d, 'rick@example.com', code, now), { ok: false, error: 'expired' }, 'dead after five wrong');
  await requestCode(d, 'rick@example.com', '1.1.1.1', now + 61_000);
  assert.deepEqual(verifyCode(d, 'rick@example.com', sent[1].code, now + 61_000 + CODE_MS + 1), { ok: false, error: 'expired' });
});
```

- [ ] **Step 2:** run → FAIL.

- [ ] **Step 3: Implement** `src/email-code.ts`:

```ts
// Emailed sign-in codes: 6 digits, 10 minutes, 5 tries, once. Sent only to an email that may get in, but
// every request gets the same answer, so the form neither says who is a member nor mails strangers.
// The table keeps HMAC(secret, email + code), never the code.
import { createHmac, randomInt, timingSafeEqual } from 'node:crypto';
import type { Db } from './db.ts';

export const CODE_MS = 10 * 60_000;
const TRIES = 5;
const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]+$/;

export type CodeDeps = { db: Db; secret: string; send: (to: string, code: string) => Promise<boolean>; mayGetIn: (email: string) => boolean };
const mac = (secret: string, email: string, code: string) => createHmac('sha256', secret).update(email + '\n' + code).digest('hex');

export async function requestCode(d: CodeDeps, raw: unknown, ip: string, now: number): Promise<{ ok: true } | { ok: false; error: 'bad-email' | 'send-failed' }> {
  const email = String(raw ?? '').trim().toLowerCase();
  if (!EMAIL.test(email) || email.length > 254) return { ok: false, error: 'bad-email' };
  const db = d.db;
  db.prepare('DELETE FROM code_requests WHERE at < ?').run(now - 86_400_000);
  const count = (sql: string, ...a: (string | number)[]) => (db.prepare(sql).get(...a) as { n: number }).n;
  const lastMinute = count('SELECT COUNT(*) AS n FROM code_requests WHERE email = ? AND at > ?', email, now - 60_000);
  const lastHour = count('SELECT COUNT(*) AS n FROM code_requests WHERE email = ? AND at > ?', email, now - 3_600_000);
  const fromIp = count('SELECT COUNT(*) AS n FROM code_requests WHERE ip = ? AND at > ?', ip, now - 3_600_000);
  if (lastMinute >= 1 || lastHour >= 5 || fromIp >= 20) return { ok: true };
  db.prepare('INSERT INTO code_requests (email, ip, at) VALUES (?, ?, ?)').run(email, ip, now);
  if (!d.mayGetIn(email)) return { ok: true };
  const code = String(randomInt(0, 1_000_000)).padStart(6, '0');
  db.prepare('INSERT INTO email_codes (email, code_hmac, expires_at, tries_left, used_at) VALUES (?, ?, ?, ?, NULL) ON CONFLICT(email) DO UPDATE SET code_hmac = excluded.code_hmac, expires_at = excluded.expires_at, tries_left = excluded.tries_left, used_at = NULL')
    .run(email, mac(d.secret, email, code), now + CODE_MS, TRIES);
  return (await d.send(email, code)) ? { ok: true } : { ok: false, error: 'send-failed' };
}

export function verifyCode(d: CodeDeps, raw: unknown, rawCode: unknown, now: number): { ok: true; email: string } | { ok: false; error: 'expired' } | { ok: false; error: 'wrong'; triesLeft: number } {
  const email = String(raw ?? '').trim().toLowerCase();
  const code = String(rawCode ?? '').replace(/\s/g, '');
  const row = d.db.prepare('SELECT code_hmac, expires_at, tries_left, used_at FROM email_codes WHERE email = ?').get(email) as
    | { code_hmac: string; expires_at: number; tries_left: number; used_at: number | null }
    | undefined;
  if (!row || row.used_at !== null || row.expires_at <= now || row.tries_left <= 0) return { ok: false, error: 'expired' };
  const given = Buffer.from(mac(d.secret, email, code));
  if (/^\d{6}$/.test(code) && timingSafeEqual(given, Buffer.from(row.code_hmac))) {
    d.db.prepare('UPDATE email_codes SET used_at = ? WHERE email = ?').run(now, email);
    return { ok: true, email };
  }
  const left = row.tries_left - 1;
  d.db.prepare('UPDATE email_codes SET tries_left = ? WHERE email = ?').run(left, email);
  return left > 0 ? { ok: false, error: 'wrong', triesLeft: left } : { ok: false, error: 'expired' };
}
```

Note: the test's last-wrong-try expects `expired` on the 5th wrong try (left becomes 0).

- [ ] **Step 4:** add to `src/members.ts` right after `mayLoadApp`:

```ts
/** Whether an emailed code may be sent to this address: the same people `mayLoadApp` lets in. */
export function mayGetIn(db: Db, email: string, opts: { adminEmail: string; now: number }): boolean {
  return mayLoadApp(db, { email, name: '', provider: null, subject: '' }, opts) === 'yes';
}
```

Run test → pass; `npm test` all pass.
- [ ] **Step 5: Commit** `git add -A src tests && git commit -m "server: emailed codes"`

---

### Task 5: Mailer sends codes from support@cavecad.com

**Goal:** The server can mail a code; the Apps Script sends invites and codes from support@cavecad.com.

**Files:**
- Modify: `apps/server/src/mailer.ts`, `apps/server/tests/mailer.test.ts`
- Modify: `tools/invite-mailer/Logic.gs`, `Code.gs`, `Code.combined.gs` (regenerate = Logic.gs + Code.gs), `test.mjs`, `README.md`

**Acceptance Criteria:**
- [ ] `sendCode(c, {to, code})` posts `{key, kind:'code', to, code}` and returns true only on `{ok:true}`
- [ ] `InviteLogic.check` accepts `kind:'code'` with a 6-digit `code` and no link; refuses a code that isn't 6 digits (`"code"`)
- [ ] `InviteLogic.message` for a code: subject `Your CaveCAD sign-in code: 123456`
- [ ] Code.gs sends with `GmailApp.sendEmail(..., { from: 'support@cavecad.com', name: 'CaveCAD' })`
- [ ] Invite email body says "Sign in with Google or an emailed code"

**Verify:** `cd apps/server && node --test tests/mailer.test.ts` and `node tools/invite-mailer/test.mjs` → all ok

**Steps:**

- [ ] **Step 1: Tests.** Append to `apps/server/tests/mailer.test.ts`:

```ts
test('sendCode posts the code kind', async () => {
  let body: Record<string, unknown> = {};
  const f = (async (_u: string, init: { body: string }) => { body = JSON.parse(init.body); return { json: async () => ({ ok: true }) }; }) as unknown as typeof fetch;
  assert.equal(await sendCode({ url: 'https://m', key: 'K', fetch: f }, { to: 'a@b.com', code: '123456' }), true);
  assert.deepEqual(body, { key: 'K', kind: 'code', to: 'a@b.com', code: '123456' });
  assert.equal(await sendCode({ url: '', key: '' }, { to: 'a@b.com', code: '123456' }), false);
});
```

(add `sendCode` to the file's import from `../src/mailer.ts`). Append to `tools/invite-mailer/test.mjs` before the final `console.log`:

```js
const cg = { key: 'K', expected: 'K', kind: 'code', to: 'rick@example.com', code: '123456', site, countThisHour: 0 };
ok(L.check(cg) === null, 'a code request passes');
ok(L.check({ ...cg, code: '12345' }) === 'code', 'code must be 6 digits');
ok(L.check({ ...cg, code: 'abcdef' }) === 'code', 'code must be digits');
const cm = L.message(cg);
ok(cm.subject === 'Your CaveCAD sign-in code: 123456', 'code subject');
ok(cm.body.includes('123456') && cm.body.includes('10 minutes'), 'code body');
ok(m.body.includes('Google or an emailed code'), 'invite says both ways in');
```

- [ ] **Step 2:** run both → FAIL.

- [ ] **Step 3: Implement.** In `mailer.ts` add:

```ts
/** Emails a sign-in code. True when the email went. */
export async function sendCode(c: MailerConfig, p: { to: string; code: string }): Promise<boolean> {
  if (!c.url || !c.key) return false;
  try {
    const res = await (c.fetch ?? fetch)(c.url, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ key: c.key, kind: 'code', ...p }), signal: AbortSignal.timeout(c.timeoutMs ?? 10_000) });
    return ((await res.json()) as { ok?: boolean }).ok === true;
  } catch {
    return false;
  }
}
```

In `Logic.gs` replace `check` and `message`:

```js
  /** null when the request may be sent, else "key" | "address" | "link" | "code" | "busy". */
  check: function (p) {
    if (!p.expected || p.key !== p.expected) return "key";
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(String(p.to || ""))) return "address";
    if (p.kind === "code") {
      if (!/^\d{6}$/.test(String(p.code || ""))) return "code";
    } else if (String(p.link || "").indexOf(p.site + "/?signup=") !== 0) return "link";
    if (p.countThisHour >= InviteLogic.MAX_PER_HOUR) return "busy";
    return null;
  },
```

```js
  /** The email itself: plain text, no tracking. */
  message: function (p) {
    if (p.kind === "code") {
      return {
        subject: "Your CaveCAD sign-in code: " + p.code,
        body: "Your CaveCAD sign-in code is " + p.code + ".\n\n" +
          "It works for 10 minutes. If you didn't ask for it, you can ignore this email.\n"
      };
    }
    var inviter = String(p.inviter || "Someone").slice(0, 60);
    return {
      subject: inviter + " invited you to CaveCAD",
      body: inviter + " invited you to CaveCAD, the cave survey and mapping app.\n\n" +
        "Sign in as " + p.to + " with Google or an emailed code. The link works once, for that address, for 7 days.\n\n" +
        p.link + "\n\n" +
        "CaveCAD stores caves on its server and syncs them to your devices.\n"
    };
  }
```

In `Code.gs` replace the send line and `setUp`'s permission line:

```js
    GmailApp.sendEmail(p.to, m.subject, m.body, { from: "support@cavecad.com", name: "CaveCAD" });
```

```js
  GmailApp.getAliases(); // asks for the Gmail permission now, not on the first email
```

and pass `kind: p.kind, code: p.code` into the `check` call object. Regenerate `Code.combined.gs`: `cat Logic.gs Code.gs > Code.combined.gs` (keep its header comment lines if the existing file has them at the top). In README step 3 change the permission wording to "approve the permission (send email as you, including the support@cavecad.com alias)".

- [ ] **Step 4:** both tests pass.
- [ ] **Step 5: Commit** `git add -A apps/server tools/invite-mailer && git commit -m "mailer: sign-in codes, sent from support@cavecad.com"`

---

### Task 6: google.ts

**Goal:** Google sign-in done by the CaveCAD server (OAuth 2 + PKCE, verified ID token).

**Files:**
- Create: `apps/server/src/google.ts`
- Test: `apps/server/tests/google.test.ts`

**Acceptance Criteria:**
- [ ] `googleStart` redirects to `https://accounts.google.com/o/oauth2/v2/auth` with client_id, redirect_uri, `code_challenge_method=S256`, state; sets a signed `cavecad_oauth` cookie (Path=/auth/google, Max-Age=600)
- [ ] `googleFinish` with matching state and a good ID token (email_verified true) → `{ ok: true, who, returnTo, desktop }`
- [ ] Mismatched/missing/tampered state cookie → `failed`; `error=access_denied` → `cancelled`; unverified email → `unverified`; bad token → `failed`
- [ ] `returnTo` only keeps same-site paths (`/x`, not `//evil` or `https://evil`)

**Verify:** `node --test tests/google.test.ts` → all pass

**Steps:**

- [ ] **Step 1: Test** — `tests/google.test.ts`:

```ts
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generateKeyPairSync, sign } from 'node:crypto';
import { googleStart, googleFinish, safeReturn, type GoogleConfig } from '../src/google.ts';

const { privateKey, publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
const jwk = { ...(publicKey.export({ format: 'jwk' }) as object), kid: 'g1', alg: 'RS256', use: 'sig' };
const now = 1_800_000_000_000;
const idToken = (claims: object) => {
  const h = Buffer.from(JSON.stringify({ alg: 'RS256', kid: 'g1' })).toString('base64url');
  const c = Buffer.from(JSON.stringify({ iss: 'https://accounts.google.com', aud: 'cid', exp: now / 1000 + 600, sub: 'g-1', email: 'Rick@gmail.com', email_verified: true, name: 'Rick', ...claims })).toString('base64url');
  return `${h}.${c}.${sign('sha256', Buffer.from(h + '.' + c), privateKey).toString('base64url')}`;
};
const cfg = (token: string): GoogleConfig => ({
  clientId: 'cid', clientSecret: 'secret', redirectUri: 'https://cavecad.test/auth/google/callback', stateSecret: 'state-secret-0123456789',
  keys: async () => [jwk],
  fetch: (async () => ({ ok: true, json: async () => ({ id_token: token }) })) as unknown as typeof fetch,
});
const cookieOf = (setCookie: string) => setCookie.split(';')[0];

test('start then finish', async () => {
  const c = cfg(idToken({}));
  const s = googleStart(c, { returnTo: '/cave/1', desktop: null }, now);
  const u = new URL(s.location);
  assert.equal(u.origin + u.pathname, 'https://accounts.google.com/o/oauth2/v2/auth');
  assert.equal(u.searchParams.get('client_id'), 'cid');
  assert.equal(u.searchParams.get('code_challenge_method'), 'S256');
  assert.match(s.cookie, /^cavecad_oauth=.+; HttpOnly; Secure; SameSite=Lax; Path=\/auth\/google; Max-Age=600$/);
  const q = new URLSearchParams({ code: 'c1', state: u.searchParams.get('state')! });
  const out = await googleFinish(c, q, cookieOf(s.cookie), now);
  assert.deepEqual(out, { ok: true, who: { email: 'rick@gmail.com', name: 'Rick', provider: 'google', subject: 'g-1' }, returnTo: '/cave/1', desktop: null });
});

test('failures', async () => {
  const c = cfg(idToken({}));
  const s = googleStart(c, { returnTo: '/', desktop: null }, now);
  const state = new URL(s.location).searchParams.get('state')!;
  assert.deepEqual(await googleFinish(c, new URLSearchParams({ code: 'c', state: 'other' }), cookieOf(s.cookie), now), { ok: false, error: 'failed' });
  assert.deepEqual(await googleFinish(c, new URLSearchParams({ code: 'c', state }), '', now), { ok: false, error: 'failed' });
  assert.deepEqual(await googleFinish(c, new URLSearchParams({ code: 'c', state }), cookieOf(s.cookie) + 'x', now), { ok: false, error: 'failed' });
  assert.deepEqual(await googleFinish(c, new URLSearchParams({ error: 'access_denied', state }), cookieOf(s.cookie), now), { ok: false, error: 'cancelled' });
  assert.deepEqual(await googleFinish(c, new URLSearchParams({ code: 'c', state }), cookieOf(s.cookie), now + 700_000), { ok: false, error: 'failed' }, 'state cookie expired');
  const unverified = cfg(idToken({ email_verified: false }));
  const s2 = googleStart(unverified, { returnTo: '/', desktop: null }, now);
  assert.deepEqual(await googleFinish(unverified, new URLSearchParams({ code: 'c', state: new URL(s2.location).searchParams.get('state')! }), cookieOf(s2.cookie), now), { ok: false, error: 'unverified' });
  const bad = cfg(idToken({ aud: 'someone-else' }));
  const s3 = googleStart(bad, { returnTo: '/', desktop: null }, now);
  assert.deepEqual(await googleFinish(bad, new URLSearchParams({ code: 'c', state: new URL(s3.location).searchParams.get('state')! }), cookieOf(s3.cookie), now), { ok: false, error: 'failed' });
});

test('safeReturn', () => {
  assert.equal(safeReturn('/cave/1?x=2'), '/cave/1?x=2');
  assert.equal(safeReturn('//evil.com'), '/');
  assert.equal(safeReturn('https://evil.com'), '/');
  assert.equal(safeReturn('/\\evil.com'), '/');
  assert.equal(safeReturn(undefined), '/');
});
```

- [ ] **Step 2:** run → FAIL.

- [ ] **Step 3: Implement** `src/google.ts`:

```ts
// Google sign-in done by the CaveCAD server: OAuth 2 authorization code with PKCE. The state, the PKCE
// verifier and where to go afterwards ride in a short-lived cookie signed with HMAC, so nothing is stored.
// The ID token is checked against Google's published keys, and the email must be one Google verified.
import { createHash, createHmac, randomBytes, timingSafeEqual, type JsonWebKey } from 'node:crypto';
import { verifyJwt, jwksKeys } from './jwt.ts';
import { readCookie } from './sessions.ts';
import type { Identity } from './members.ts';

const AUTH = 'https://accounts.google.com/o/oauth2/v2/auth';
const TOKEN = 'https://oauth2.googleapis.com/token';
const JWKS = 'https://www.googleapis.com/oauth2/v3/certs';
const ISSUERS = ['https://accounts.google.com', 'accounts.google.com'];
const COOKIE = 'cavecad_oauth';
const LIFE_MS = 600_000;

export type Desktop = { challenge: string; state: string } | null;
export type GoogleConfig = { clientId: string; clientSecret: string; redirectUri: string; stateSecret: string; fetch?: typeof fetch; keys?: (kid?: string) => Promise<JsonWebKey[]> };
type Kept = { state: string; verifier: string; returnTo: string; desktop: Desktop; exp: number };

const b64 = (b: Buffer) => b.toString('base64url');
const macOf = (secret: string, body: string) => b64(createHmac('sha256', secret).update(body).digest());

/** A path on this site to go back to; anything else becomes '/'. */
export function safeReturn(v: unknown): string {
  const s = typeof v === 'string' ? v : '';
  return s.startsWith('/') && !s.startsWith('//') && !s.startsWith('/\\') ? s : '/';
}

let defaultKeys: ((kid?: string) => Promise<JsonWebKey[]>) | null = null;

export function googleStart(c: GoogleConfig, p: { returnTo: string; desktop: Desktop }, now: number): { location: string; cookie: string } {
  const verifier = b64(randomBytes(32));
  const kept: Kept = { state: b64(randomBytes(16)), verifier, returnTo: safeReturn(p.returnTo), desktop: p.desktop, exp: now + LIFE_MS };
  const body = b64(Buffer.from(JSON.stringify(kept)));
  const u = new URL(AUTH);
  for (const [k, v] of Object.entries({ response_type: 'code', client_id: c.clientId, redirect_uri: c.redirectUri, scope: 'openid email profile', state: kept.state, code_challenge: b64(createHash('sha256').update(verifier).digest()), code_challenge_method: 'S256', prompt: 'select_account' })) u.searchParams.set(k, v);
  return { location: u.toString(), cookie: `${COOKIE}=${body}.${macOf(c.stateSecret, body)}; HttpOnly; Secure; SameSite=Lax; Path=/auth/google; Max-Age=${LIFE_MS / 1000}` };
}

function readKept(c: GoogleConfig, cookieHeader: string | undefined, now: number): Kept | null {
  const raw = readCookie(cookieHeader, COOKIE);
  const dot = raw.lastIndexOf('.');
  if (dot < 1) return null;
  const body = raw.slice(0, dot);
  const a = Buffer.from(raw.slice(dot + 1));
  const b = Buffer.from(macOf(c.stateSecret, body));
  if (a.length !== b.length || !timingSafeEqual(a, b)) return null;
  try {
    const k = JSON.parse(Buffer.from(body, 'base64url').toString()) as Kept;
    return k.exp > now ? k : null;
  } catch {
    return null;
  }
}

export const clearOauthCookie = `${COOKIE}=; HttpOnly; Secure; SameSite=Lax; Path=/auth/google; Max-Age=0`;

export async function googleFinish(c: GoogleConfig, q: URLSearchParams, cookieHeader: string | undefined, now: number): Promise<{ ok: true; who: Identity; returnTo: string; desktop: Desktop } | { ok: false; error: 'cancelled' | 'failed' | 'unverified' }> {
  const kept = readKept(c, cookieHeader, now);
  if (!kept || q.get('state') !== kept.state) return { ok: false, error: 'failed' };
  if (q.get('error')) return { ok: false, error: 'cancelled' };
  const code = q.get('code');
  if (!code) return { ok: false, error: 'failed' };
  try {
    const form = new URLSearchParams({ grant_type: 'authorization_code', code, redirect_uri: c.redirectUri, client_id: c.clientId, client_secret: c.clientSecret, code_verifier: kept.verifier });
    const res = await (c.fetch ?? fetch)(TOKEN, { method: 'POST', headers: { 'Content-Type': 'application/x-www-form-urlencoded' }, body: form.toString(), signal: AbortSignal.timeout(10_000) });
    const out = (await res.json()) as { id_token?: string };
    if (!res.ok || !out.id_token) return { ok: false, error: 'failed' };
    const keys = c.keys ?? (defaultKeys ??= jwksKeys(JWKS));
    const claims = await verifyJwt(out.id_token, { keys, issuers: ISSUERS, audience: c.clientId, now: Math.floor(now / 1000) });
    if (!claims || typeof claims.email !== 'string') return { ok: false, error: 'failed' };
    if (claims.email_verified !== true) return { ok: false, error: 'unverified' };
    const email = claims.email.toLowerCase();
    return { ok: true, who: { email, name: typeof claims.name === 'string' ? claims.name : email.split('@')[0], provider: 'google', subject: String(claims.sub ?? '') }, returnTo: kept.returnTo, desktop: kept.desktop };
  } catch {
    return { ok: false, error: 'failed' };
  }
}
```

- [ ] **Step 4:** test passes.
- [ ] **Step 5: Commit** `git add src/google.ts tests/google.test.ts && git commit -m "server: Google sign-in without TinyAuth"`

---

### Task 7: handoff.ts (desktop)

**Goal:** A browser sign-in hands the desktop app a session through a one-time PKCE-bound code.

**Files:**
- Create: `apps/server/src/handoff.ts`
- Test: `apps/server/tests/handoff.test.ts`

**Acceptance Criteria:**
- [ ] `createHandoff(db, who, challenge, now)` returns a code; `redeemHandoff(db, code, verifier, now)` returns the Identity once when `base64url(sha256(verifier)) === challenge`
- [ ] Wrong verifier, second use, after 60 s → null

**Verify:** `node --test tests/handoff.test.ts` → all pass

**Steps:**

- [ ] **Step 1: Test:**

```ts
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { openDb } from '../src/db.ts';
import { createHandoff, redeemHandoff } from '../src/handoff.ts';

const who = { email: 'rick@example.com', name: 'Rick', provider: 'email' as const, subject: 'session' };
const verifier = 'v'.repeat(43);
const challenge = createHash('sha256').update(verifier).digest('base64url');
const now = 1_800_000_000_000;

test('redeems once with the right verifier', () => {
  const db = openDb(':memory:');
  const code = createHandoff(db, who, challenge, now);
  assert.equal(redeemHandoff(db, code, 'wrong', now), null);
  assert.deepEqual(redeemHandoff(db, code, verifier, now), who);
  assert.equal(redeemHandoff(db, code, verifier, now), null);
});
test('expires after a minute', () => {
  const db = openDb(':memory:');
  const code = createHandoff(db, who, challenge, now);
  assert.equal(redeemHandoff(db, code, verifier, now + 61_000), null);
});
```

- [ ] **Step 2:** FAIL.
- [ ] **Step 3: Implement** `src/handoff.ts`:

```ts
// The desktop app's sign-in hand-off: after someone signs in in the browser, the server gives the browser a
// one-time code for cavecad://auth. Only the app that made the PKCE challenge can redeem it (its verifier), once,
// within a minute; it gets a desktop session back.
import { createHash, randomBytes } from 'node:crypto';
import type { Db } from './db.ts';
import type { Identity } from './members.ts';

const hash = (t: string) => createHash('sha256').update(t).digest('hex');

export function createHandoff(db: Db, who: Identity, challenge: string, now: number): string {
  const code = randomBytes(24).toString('base64url');
  db.prepare('DELETE FROM handoffs WHERE expires_at < ?').run(now - 3_600_000);
  db.prepare('INSERT INTO handoffs (code_hash, challenge, email, name, provider, expires_at) VALUES (?, ?, ?, ?, ?, ?)').run(hash(code), challenge, who.email, who.name, who.provider, now + 60_000);
  return code;
}

export function redeemHandoff(db: Db, code: string, verifier: string, now: number): Identity | null {
  const row = db.prepare('SELECT challenge, email, name, provider, expires_at, used_at FROM handoffs WHERE code_hash = ?').get(hash(String(code))) as
    | { challenge: string; email: string; name: string; provider: Identity['provider']; expires_at: number; used_at: number | null }
    | undefined;
  if (!row || row.used_at !== null || row.expires_at <= now) return null;
  if (createHash('sha256').update(String(verifier)).digest('base64url') !== row.challenge) return null;
  db.prepare('UPDATE handoffs SET used_at = ? WHERE code_hash = ?').run(now, hash(String(code)));
  return { email: row.email, name: row.name, provider: row.provider, subject: 'session' };
}
```

- [ ] **Step 4:** pass. **Step 5: Commit** `git add src/handoff.ts tests/handoff.test.ts && git commit -m "server: desktop sign-in hand-off"`

---

### Task 8: identity.ts on sessions

**Goal:** `whoIs` believes only CaveCAD sessions (plus tailnet/dev), so forged Remote-* headers and TinyAuth tokens mean nothing.

**Files:**
- Rewrite: `apps/server/src/identity.ts`
- Rewrite: `apps/server/tests/identity.test.ts`

**Acceptance Criteria:**
- [ ] Cookie `cavecad_session=<live token>` → Identity; `Authorization: Bearer <live desktop token>` → Identity
- [ ] Remote-Email + X-Cave-Gate headers alone → null
- [ ] Revoked/expired token → null
- [ ] Tailnet and dev identities behave as before (127.0.0.1 only)

**Verify:** `node --test tests/identity.test.ts` → all pass

**Steps:**

- [ ] **Step 1: Test** — replace `tests/identity.test.ts` with:

```ts
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { openDb } from '../src/db.ts';
import { createSession, revokeSession } from '../src/sessions.ts';
import { makeWhoIs, type RequestLike } from '../src/identity.ts';

const now = 1_800_000_000_000;
const req = (addr: string, headers: Record<string, string>): RequestLike => ({ socket: { remoteAddress: addr }, headers });
const who = { email: 'rick@example.com', name: 'Rick', provider: 'email' as const, subject: '' };

test('a session cookie or bearer token says who it is', async () => {
  const db = openDb(':memory:');
  const whoIs = makeWhoIs({ db, now: () => now });
  const web = createSession(db, who, 'web', now);
  const desk = createSession(db, who, 'desktop', now);
  assert.equal((await whoIs(req('1.2.3.4', { cookie: `x=1; cavecad_session=${web}` })))?.email, 'rick@example.com');
  assert.equal((await whoIs(req('1.2.3.4', { authorization: `Bearer ${desk}` })))?.email, 'rick@example.com');
  revokeSession(db, web, now);
  assert.equal(await whoIs(req('1.2.3.4', { cookie: `cavecad_session=${web}` })), null);
});
test('edge headers are no longer believed', async () => {
  const db = openDb(':memory:');
  const whoIs = makeWhoIs({ db, now: () => now });
  assert.equal(await whoIs(req('192.168.86.13', { 'x-cave-gate': 'anything', 'remote-email': 'rick@example.com' })), null);
});
test('tailnet and dev identities work from this machine only', async () => {
  const db = openDb(':memory:');
  const t = makeWhoIs({ db, tailnet: true, now: () => now });
  assert.equal((await t(req('127.0.0.1', { 'tailscale-user-login': 'Rick@Example.com', 'tailscale-user-name': '=?utf-8?q?Jos=C3=A9?=' })))?.name, 'José');
  assert.equal(await t(req('10.0.0.2', { 'tailscale-user-login': 'rick@example.com' })), null);
  const d = makeWhoIs({ db, devIdentity: 'dev@example.com', now: () => now });
  assert.equal((await d(req('::1', {})))?.email, 'dev@example.com');
  assert.equal(await d(req('10.0.0.2', {})), null);
});
```

- [ ] **Step 2:** FAIL.

- [ ] **Step 3: Implement** — replace `src/identity.ts` with:

```ts
// The identity adapter: the one place that decides who a request is. Since 2026-11 CaveCAD signs people in
// itself (signin.ts): a browser carries a session cookie, the desktop app a bearer session token. A private
// copy reached through `tailscale serve`, and local development, have their own identities.
import type { Db } from './db.ts';
import type { Identity } from './members.ts';
import { findSession, readCookie, SESSION_COOKIE } from './sessions.ts';

export type RequestLike = { socket: { remoteAddress?: string }; headers: Record<string, string | string[] | undefined> };
export type WhoIsOptions = {
  db: Db;
  /** Reached through `tailscale serve` on this machine: its Tailscale-User-* headers are believed from 127.0.0.1 alone. */
  tailnet?: boolean;
  /** For local development only: everyone on this machine is this email. */
  devIdentity?: string;
  now?: () => number;
};

const plain = (addr = '') => addr.replace(/^::ffff:/, '');
const header = (r: RequestLike, name: string) => {
  const v = r.headers[name];
  return (Array.isArray(v) ? v[0] : v)?.trim() ?? '';
};
/** A header value tailscale serve wrote: plain ASCII, or RFC 2047 Q-encoded UTF-8 (`=?utf-8?q?Jos=C3=A9?=`). */
function qDecoded(v: string): string {
  const m = /^=\?utf-8\?q\?(.*)\?=$/i.exec(v);
  if (!m) return v;
  const bytes = m[1].replace(/_/g, ' ').replace(/=([0-9A-Fa-f]{2})/g, (_, h: string) => String.fromCharCode(parseInt(h, 16)));
  return Buffer.from(bytes, 'latin1').toString('utf8');
}
const local = (addr: string) => addr === '127.0.0.1' || addr === '::1';

/** Builds `whoIs(request) -> Identity | null` for these settings. */
export function makeWhoIs(o: WhoIsOptions): (r: RequestLike) => Promise<Identity | null> {
  const now = o.now ?? Date.now;
  return async (r) => {
    const addr = plain(r.socket.remoteAddress);
    const bearer = /^Bearer (\S+)$/.exec(header(r, 'authorization'))?.[1];
    if (bearer) return findSession(o.db, bearer, now());
    const cookie = readCookie(header(r, 'cookie'), SESSION_COOKIE);
    if (cookie) {
      const found = findSession(o.db, cookie, now());
      if (found) return found;
    }
    if (o.tailnet && local(addr)) {
      const email = header(r, 'tailscale-user-login').toLowerCase();
      if (!email) return null;
      return { email, name: qDecoded(header(r, 'tailscale-user-name')) || email.split('@')[0], provider: null, subject: 'tailnet:' + email };
    }
    if (o.devIdentity && local(addr)) return { email: o.devIdentity.toLowerCase(), name: o.devIdentity.split('@')[0], provider: null, subject: 'dev' };
    return null;
  };
}
```

- [ ] **Step 4:** identity tests pass. `main.ts` will not compile yet (fixed in Task 10); `npm test` may fail only on files importing `discoveredKeys`/`desktop-token` — delete `tests/desktop-token.test.ts` and `src/desktop-token.ts` now (`git rm`).
- [ ] **Step 5: Commit** `git add -A src tests && git commit -m "server: identity from CaveCAD sessions"`

---

### Task 9: signin.ts (routes and page)

**Goal:** The sign-in page and routes: codes, Google, sign-out, desktop hand-off.

**Files:**
- Create: `apps/server/src/signin.ts`
- Test: `apps/server/tests/signin.test.ts`

**Acceptance Criteria:**
- [ ] `GET /signin` → 200 HTML with "Continue with Google" and "Email me a code"
- [ ] `POST /signin/code` (JSON, same-origin) → `{ok:true}` for anyone; cross-site (`Sec-Fetch-Site: cross-site`) → 403
- [ ] `POST /signin/verify` right code → `{ next: '/' }` and `Set-Cookie: cavecad_session=…`; wrong → 400 `{error:'wrong', triesLeft}`; expired → 400 `{error:'expired'}`
- [ ] With `desktop=1&challenge&state` the verify answer is `{ next: 'cavecad://auth?code=…&state=…' }` and no cookie
- [ ] `GET /auth/google` → 302 to Google + oauth cookie; `GET /auth/google/callback` success → 302 to returnTo with session cookie; failure → 302 `/signin?error=google`
- [ ] `POST /signout` → revokes, clears cookie, `{ok:true}`; `GET /signout` → 302 `/signin` after revoking

**Verify:** `node --test tests/signin.test.ts` → all pass

**Steps:**

- [ ] **Step 1: Test** — `tests/signin.test.ts`:

```ts
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createHash } from 'node:crypto';
import { openDb } from '../src/db.ts';
import { handleSignin, type SigninDeps } from '../src/signin.ts';
import { findSession } from '../src/sessions.ts';
import { redeemHandoff } from '../src/handoff.ts';

async function start() {
  const db = openDb(':memory:');
  const sent: { to: string; code: string }[] = [];
  const d: SigninDeps = {
    db, publicUrl: 'https://cavecad.test', secure: true,
    codes: { db, secret: 'secret-0123456789abcdef', send: async (to, code) => (sent.push({ to, code }), true), mayGetIn: (e) => e === 'rick@example.com' },
    google: null,
  };
  const server = createServer((req, res) => void handleSignin(d, req, res).then((done) => { if (!done) res.writeHead(404).end(); }));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const post = (path: string, body: object, h: Record<string, string> = {}) => fetch(base + path, { method: 'POST', headers: { 'Content-Type': 'application/json', 'Sec-Fetch-Site': 'same-origin', ...h }, body: JSON.stringify(body), redirect: 'manual' });
  return { db, sent, server, base, post };
}

test('page, code, verify, sign out', async () => {
  const { db, sent, server, base, post } = await start();
  try {
    const page = await (await fetch(base + '/signin')).text();
    assert.ok(page.includes('Continue with Google') && page.includes('Email me a code'));
    assert.equal((await post('/signin/code', { email: 'rick@example.com' }, { 'Sec-Fetch-Site': 'cross-site' })).status, 403);
    assert.deepEqual(await (await post('/signin/code', { email: 'rick@example.com' })).json(), { ok: true });
    const wrong = sent[0].code === '000000' ? '111111' : '000000';
    const bad = await post('/signin/verify', { email: 'rick@example.com', code: wrong });
    assert.equal(bad.status, 400);
    assert.deepEqual(await bad.json(), { error: 'wrong', triesLeft: 4 });
    const good = await post('/signin/verify', { email: 'rick@example.com', code: sent[0].code, return: '/x' });
    assert.deepEqual(await good.json(), { next: '/x' });
    const cookie = good.headers.get('set-cookie') ?? '';
    const token = /cavecad_session=([^;]+)/.exec(cookie)![1];
    assert.equal(findSession(db, token, Date.now())?.email, 'rick@example.com');
    const out = await post('/signout', {}, { Cookie: `cavecad_session=${token}` });
    assert.match(out.headers.get('set-cookie') ?? '', /Max-Age=0/);
    assert.equal(findSession(db, token, Date.now()), null);
  } finally {
    server.close();
  }
});

test('desktop hand-off', async () => {
  const { db, sent, server, post } = await start();
  try {
    const verifier = 'w'.repeat(43);
    const challenge = createHash('sha256').update(verifier).digest('base64url');
    await post('/signin/code', { email: 'rick@example.com' });
    const r = await post('/signin/verify', { email: 'rick@example.com', code: sent[0].code, desktop: '1', challenge, state: 'st' });
    assert.equal(r.headers.get('set-cookie'), null);
    const next = new URL(((await r.json()) as { next: string }).next);
    assert.equal(next.protocol + '//' + next.host, 'cavecad://auth');
    assert.equal(next.searchParams.get('state'), 'st');
    assert.equal(redeemHandoff(db, next.searchParams.get('code')!, verifier, Date.now())?.email, 'rick@example.com');
  } finally {
    server.close();
  }
});

test('the desktop done page only offers a cavecad:// link', async () => {
  const { server, base } = await start();
  try {
    const evil = await (await fetch(base + '/signin?done=desktop&next=' + encodeURIComponent('javascript:alert(1)'))).text();
    assert.ok(!evil.includes('javascript:alert'));
    const good = await (await fetch(base + '/signin?done=desktop&next=' + encodeURIComponent('cavecad://auth?code=abc&state=st'))).text();
    assert.ok(good.includes("You're signed in"));
  } finally {
    server.close();
  }
});
```

- [ ] **Step 2:** FAIL.

- [ ] **Step 3: Implement** `src/signin.ts`:

```ts
// CaveCAD's sign-in: a small page (no app in it) with Google and an emailed code, the routes behind it, and
// sign-out. Signing in ends in a web session cookie, or, for the desktop app, a hand-off code it redeems.
import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Db } from './db.ts';
import type { Identity } from './members.ts';
import { send, sendBadBody, readJson } from './http.ts';
import { requestCode, verifyCode, type CodeDeps } from './email-code.ts';
import { googleStart, googleFinish, clearOauthCookie, safeReturn, type GoogleConfig, type Desktop } from './google.ts';
import { createSession, revokeSession, sessionCookie, clearSessionCookie, readCookie, SESSION_COOKIE } from './sessions.ts';
import { createHandoff } from './handoff.ts';

export type SigninDeps = { db: Db; publicUrl: string; secure: boolean; codes: CodeDeps; google: GoogleConfig | null; now?: () => number };

const B64URL = /^[A-Za-z0-9_-]{16,128}$/;
function desktopOf(v: { desktop?: unknown; challenge?: unknown; state?: unknown }): Desktop {
  if (String(v.desktop ?? '') !== '1') return null;
  const challenge = String(v.challenge ?? '');
  const state = String(v.state ?? '');
  return B64URL.test(challenge) && /^[A-Za-z0-9_-]{1,128}$/.test(state) ? { challenge, state } : null;
}

/** Where to go once signed in, plus the cookie to set (none for the desktop). */
function finish(d: SigninDeps, who: Identity, returnTo: string, desktop: Desktop, now: number): { next: string; cookie: string | null } {
  if (desktop) {
    const code = createHandoff(d.db, who, desktop.challenge, now);
    return { next: `cavecad://auth?code=${encodeURIComponent(code)}&state=${encodeURIComponent(desktop.state)}`, cookie: null };
  }
  return { next: safeReturn(returnTo), cookie: sessionCookie(createSession(d.db, who, 'web', now), d.secure) };
}

/** JSON posts from this site only (a plain form or another site cannot send them). */
function crossSite(req: IncomingMessage): boolean {
  const type = String(req.headers['content-type'] ?? '').toLowerCase();
  const site = req.headers['sec-fetch-site'];
  return !type.startsWith('application/json') || (site !== undefined && site !== 'same-origin' && site !== 'none');
}

const ipOf = (req: IncomingMessage) => String(req.headers['x-forwarded-for'] ?? '').split(',')[0].trim() || (req.socket.remoteAddress ?? '');

/** Handles a sign-in route; false when the path is not one. */
export async function handleSignin(d: SigninDeps, req: IncomingMessage, res: ServerResponse): Promise<boolean> {
  const url = new URL(req.url ?? '/', 'http://x');
  const path = url.pathname;
  const method = req.method ?? 'GET';
  const now = (d.now ?? Date.now)();
  try {
    if (path === '/signin' && method === 'GET') {
      res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8', 'Cache-Control': 'no-store' }).end(page(url.searchParams, !!d.google));
      return true;
    }
    if ((path === '/signin/code' || path === '/signin/verify' || path === '/signout') && method === 'POST' && crossSite(req)) return send(res, 403, { error: 'cross-site' }), true;
    if (path === '/signin/code' && method === 'POST') {
      const b = await readJson(req);
      const r = await requestCode(d.codes, b.email, ipOf(req), now);
      return send(res, r.ok ? 200 : 400, r.ok ? { ok: true } : { error: r.error }), true;
    }
    if (path === '/signin/verify' && method === 'POST') {
      const b = await readJson(req);
      const r = verifyCode(d.codes, b.email, b.code, now);
      if (!r.ok) return send(res, 400, r.error === 'wrong' ? { error: 'wrong', triesLeft: r.triesLeft } : { error: 'expired' }), true;
      const f = finish(d, { email: r.email, name: r.email.split('@')[0], provider: 'email', subject: 'session' }, String(b.return ?? '/'), desktopOf(b), now);
      if (f.cookie) res.setHeader('Set-Cookie', f.cookie);
      return send(res, 200, { next: f.next }), true;
    }
    if (path === '/auth/google' && method === 'GET') {
      if (!d.google) return res.writeHead(302, { Location: '/signin' }).end(), true;
      const p = Object.fromEntries(url.searchParams);
      const s = googleStart(d.google, { returnTo: p.return ?? '/', desktop: desktopOf(p) }, now);
      res.writeHead(302, { Location: s.location, 'Set-Cookie': s.cookie, 'Cache-Control': 'no-store' }).end();
      return true;
    }
    if (path === '/auth/google/callback' && method === 'GET') {
      if (!d.google) return res.writeHead(302, { Location: '/signin' }).end(), true;
      const r = await googleFinish(d.google, url.searchParams, req.headers.cookie, now);
      if (!r.ok) {
        res.writeHead(302, { Location: `/signin?error=${r.error === 'unverified' ? 'unverified' : 'google'}`, 'Set-Cookie': clearOauthCookie }).end();
        return true;
      }
      const f = finish(d, r.who, r.returnTo, r.desktop, now);
      res.writeHead(302, { Location: r.desktop ? `/signin?done=desktop&next=${encodeURIComponent(f.next)}` : f.next, 'Set-Cookie': [clearOauthCookie, ...(f.cookie ? [f.cookie] : [])], 'Cache-Control': 'no-store' }).end();
      return true;
    }
    if (path === '/signout') {
      if (method !== 'POST' && method !== 'GET') return false;
      revokeSession(d.db, readCookie(req.headers.cookie, SESSION_COOKIE), now);
      if (method === 'POST') {
        res.setHeader('Set-Cookie', clearSessionCookie(d.secure));
        return send(res, 200, { ok: true }), true;
      }
      res.writeHead(302, { Location: '/signin', 'Set-Cookie': clearSessionCookie(d.secure) }).end();
      return true;
    }
  } catch (e) {
    if (e instanceof SyntaxError) return sendBadBody(res, e), true;
    throw e;
  }
  return false;
}

const esc = (t: string) => t.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
const ERRORS: Record<string, string> = {
  google: "Google sign-in didn't finish. Try again.",
  unverified: "Google hasn't confirmed that email. Use Email me a code instead.",
};

/** The sign-in page: plain HTML and a little script; the desktop hand-off parameters ride along. */
function page(q: URLSearchParams, google: boolean): string {
  const keep = new URLSearchParams();
  for (const k of ['return', 'desktop', 'challenge', 'state']) if (q.get(k)) keep.set(k, q.get(k)!);
  // Only a cavecad://auth link may be offered here: anything else in ?next= would be a script-injection hole.
  const nextLink = q.get('next') ?? '';
  const done = q.get('done') === 'desktop' && /^cavecad:\/\/auth\?[A-Za-z0-9_%=&.-]+$/.test(nextLink) ? nextLink : '';
  const err = ERRORS[q.get('error') ?? ''] ?? '';
  return `<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>Sign in to CaveCAD</title><link rel="icon" href="/favicon.svg"><link rel="apple-touch-icon" href="/apple-touch-icon.png">
<style>
:root{color-scheme:light dark;--bg:#f7f6f2;--card:#fff;--text:#1c2733;--muted:#5b6670;--line:#d9d6cc;--accent:#e8590c}
@media (prefers-color-scheme:dark){:root{--bg:#14181d;--card:#1c2228;--text:#e7e9ec;--muted:#9aa4ae;--line:#2c343c}}
*{box-sizing:border-box}body{margin:0;min-height:100vh;display:grid;place-items:center;padding:16px;background:var(--bg);color:var(--text);font:16px/1.5 system-ui,sans-serif}
.card{width:min(400px,100%);background:var(--card);border:1px solid var(--line);border-radius:14px;padding:24px;display:flex;flex-direction:column;gap:12px}
h1{margin:0;font-size:20px}p{margin:0}.muted{color:var(--muted);font-size:14px}.bad{color:#c92a2a;font-size:14px}
input{font:inherit;padding:10px 12px;border:1px solid var(--line);border-radius:8px;background:transparent;color:inherit;width:100%}
button,.btn{font:inherit;padding:10px 12px;border-radius:8px;border:1px solid var(--line);background:transparent;color:inherit;cursor:pointer;text-align:center;text-decoration:none;display:block;width:100%}
.primary{background:var(--accent);border-color:var(--accent);color:#fff}.or{text-align:center;color:var(--muted);font-size:13px}[hidden]{display:none!important}
#code{letter-spacing:.4em;text-align:center;font-size:22px}
</style></head><body><main class="card">
<img src="/favicon.svg" alt="" width="48" height="48">
${done ? `<h1>You're signed in</h1><p>You can go back to CaveCAD now.</p><a class="btn primary" href="${esc(done)}">Open CaveCAD</a><script>location.href=${JSON.stringify(done)}</script>` : `
<h1>Sign in to CaveCAD</h1>
${err ? `<p class="bad" role="alert">${esc(err)}</p>` : ''}
${google ? `<a class="btn" href="/auth/google?${esc(keep.toString())}">Continue with Google</a><p class="or">or</p>` : ''}
<form id="f1"><label class="muted" for="email">Your email</label><input id="email" type="email" autocomplete="email" required><button class="primary" style="margin-top:8px">Email me a code</button></form>
<form id="f2" hidden><p id="sentto"></p><input id="code" inputmode="numeric" autocomplete="one-time-code" maxlength="6" pattern="[0-9]{6}" required><button class="primary" style="margin-top:8px">Sign in</button><button type="button" id="again" style="margin-top:8px">Send a new code</button></form>
<p id="msg" class="bad" role="alert"></p>
<p class="muted">CaveCAD is invite only. <a href="/privacy.html">Privacy</a></p>
<script>
const keep=${JSON.stringify(Object.fromEntries(keep))};
const $=(id)=>document.getElementById(id);const msg=(t)=>{$('msg').textContent=t||''};
const post=async(p,b)=>{const r=await fetch(p,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(b)});return{ok:r.ok,body:await r.json().catch(()=>({}))}};
async function ask(){msg('');const email=$('email').value.trim();const r=await post('/signin/code',{email});
if(!r.ok){msg(r.body.error==='bad-email'?'That doesn\\'t look like an email address.':'We couldn\\'t send the email just now. Try again in a few minutes.');return}
$('f1').hidden=true;$('f2').hidden=false;$('sentto').textContent='If '+email+' can use CaveCAD, a 6-digit code is on its way. It works for 10 minutes.';$('code').focus()}
$('f1').onsubmit=(e)=>{e.preventDefault();ask()};
$('again').onclick=()=>{$('f2').hidden=true;$('f1').hidden=false;ask()};
$('f2').onsubmit=async(e)=>{e.preventDefault();msg('');const r=await post('/signin/verify',{email:$('email').value.trim(),code:$('code').value,...keep});
if(r.ok){location.href=r.body.next;return}
msg(r.body.error==='wrong'?'That code isn\\'t right. '+r.body.triesLeft+(r.body.triesLeft===1?' try':' tries')+' left.':'That code has expired. Ask for a new one.')};
</script>`}
</main></body></html>`;
}
```

Note for the desktop code path: when `next` is `cavecad://…` the page's `location.href = next` opens the app; the page then still shows the code form, which is fine. Improve: after a `cavecad:` next, also show "You're signed in. You can go back to CaveCAD now." — add after `if(r.ok){`:
`if(r.body.next.startsWith('cavecad:')){document.querySelector('main').innerHTML='<h1>You\\'re signed in</h1><p>You can go back to CaveCAD now.</p>'}`
before `location.href=r.body.next`.

- [ ] **Step 4:** test passes.
- [ ] **Step 5: Commit** `git add src/signin.ts tests/signin.test.ts && git commit -m "server: sign-in page and routes"`

---

### Task 10: Wire into api.ts and main.ts

**Goal:** The server runs on its own sign-in: signed-out visitors go to /signin, the desktop redeems hand-offs, removal signs a member out everywhere.

**Files:**
- Modify: `apps/server/src/api.ts`, `apps/server/src/main.ts`, `apps/server/.env.example`
- Modify: `apps/server/tests/api.test.ts`

**Acceptance Criteria:**
- [ ] Signed-out GET `/` → 302 `/signin?return=%2F`; `PUBLIC_FILES` still 200 signed out
- [ ] `POST /api/desktop-token {code, verifier}` → 200 `{token, expires_in: 2592000}` for a valid hand-off; 400 `{error:'sign-in-failed'}` otherwise
- [ ] `DELETE /api/admin/members/<id>` also revokes that member's sessions
- [ ] Not-invited/removed page offers "Use another account" → `/signout`
- [ ] `/api/desktop-config` gone (404); server starts with only `PUBLIC_URL`, `ADMIN_EMAIL`, `PASS_KEY_FILE`, `CODE_SECRET` (+ optional Google, mailer, downloads)
- [ ] `npm test` all pass

**Verify:** `cd apps/server && npm test` → `fail 0`

**Steps:**

- [ ] **Step 1: Tests** — in `tests/api.test.ts`:
  - Find assertions that expect signed-out `serveApp` → 401 HTML ("Sign in to CaveCAD") and change them to expect `302` with `location` `/signin?return=%2F` (use `redirect: 'manual'` — add `redirect: 'manual'` to the `fetch` in `call`).
  - Add:

```ts
test('desktop token from a hand-off', async () => {
  const { server, call, db } = await start();
  try {
    const { createHash } = await import('node:crypto');
    const { createHandoff } = await import('../src/handoff.ts');
    const verifier = 'z'.repeat(43);
    const code = createHandoff(db, { email: 'rick@example.com', name: 'Rick', provider: 'email', subject: 'session' }, createHash('sha256').update(verifier).digest('base64url'), Date.now());
    const ok = await call(null, 'POST', '/api/desktop-token', { code, verifier });
    assert.equal(ok.status, 200);
    assert.equal((ok.json as { expires_in: number }).expires_in, 2592000);
    assert.equal((await call(null, 'POST', '/api/desktop-token', { code, verifier })).status, 400);
    assert.equal((await call(null, 'GET', '/api/desktop-config')).status, 404);
  } finally {
    server.close();
  }
});
```

  - In the existing remove-member test, after the DELETE succeeds, assert a session made for that member's email with `createSession` before the DELETE is dead after (`findSession(...) === null`).

- [ ] **Step 2:** run → FAIL.

- [ ] **Step 3: Implement api.ts.**
  1. Imports: remove nothing else; add
     `import { createSession, revokeAllFor, SESSION_MS } from './sessions.ts';`
     `import { redeemHandoff } from './handoff.ts';`
  2. In `ApiDeps` delete `signoutUrl`, `desktopToken`, `desktopConfig` fields and their comments.
  3. Replace the whole first `try { … }` block (desktop-config/desktop-token) with:

```ts
  try {
    // The desktop app's sign-in hand-off (handoff.ts); public, so before the identity check.
    if (path === '/api/desktop-token' && method === 'POST') {
      const b = await readJson(req);
      const who = redeemHandoff(d.db, String(b.code ?? ''), String(b.verifier ?? ''), now);
      if (!who) return send(res, 400, { error: 'sign-in-failed' });
      return send(res, 200, { token: createSession(d.db, who, 'desktop', now), expires_in: SESSION_MS / 1000 });
    }
  } catch (e) {
    if (e instanceof SyntaxError) return sendBadBody(res, e);
    throw e;
  }
```

  4. Around the member-removal line (`M.removeMember(d.db, me.id, member[1], now);`): read the email before it and
     revoke after it:

```ts
      const goneEmail = (d.db.prepare('SELECT email FROM members WHERE id = ?').get(member[1]) as { email: string } | undefined)?.email;
      M.removeMember(d.db, me.id, member[1], now);
      if (goneEmail) revokeAllFor(d.db, goneEmail, now);
```
  5. `refusal()`: replace the `out` line with
     `const out = '<p><a href="/signout">Use another account</a></p>';` and drop the `d` parameter use (keep the signature).
  6. In `serveApp`, replace the `!who` line with:

```ts
  if (!who) {
    const here = (req.url ?? '/').split('#')[0];
    res.writeHead(302, { Location: '/signin?return=' + encodeURIComponent(here), 'Cache-Control': 'no-store' }).end();
    return;
  }
```

- [ ] **Step 4: Implement main.ts** — replace the file with:

```ts
// Starts the CaveCAD server: sign-in, the web app, downloads and the account API on one port. Settings come
// from the environment (see .env.example); nothing secret is in the code.
import { createServer } from 'node:http';
import { readFileSync } from 'node:fs';
import { createPrivateKey } from 'node:crypto';
import { dirname, join, resolve } from 'node:path';
import { openDb } from './db.ts';
import { handleApi, serveApp } from './api.ts';
import { makeWhoIs } from './identity.ts';
import { handleSignin } from './signin.ts';
import { handleDownloads } from './downloads.ts';
import { mayGetIn } from './members.ts';
import { sendCode } from './mailer.ts';

const env = process.env;
const need = (k: string) => env[k] || (() => { throw new Error(`${k} is not set`); })();

const publicUrl = need('PUBLIC_URL');
const devIdentity = env.DEV_IDENTITY || undefined;
if (devIdentity) {
  const host = (() => { try { return new URL(publicUrl).hostname; } catch { return ''; } })();
  if (host !== 'localhost' && host !== '127.0.0.1') throw new Error(`DEV_IDENTITY needs a localhost PUBLIC_URL, not ${publicUrl}`);
}
const tailnet = env.TAILNET_IDENTITY === '1';
if (tailnet) {
  if (devIdentity) throw new Error('DEV_IDENTITY and TAILNET_IDENTITY cannot run together');
  if (env.HOST !== '127.0.0.1') throw new Error('TAILNET_IDENTITY needs HOST=127.0.0.1');
}

const db = openDb(env.DB_PATH ?? 'cavecad.sqlite');
const passKey = createPrivateKey({ key: JSON.parse(readFileSync(need('PASS_KEY_FILE'), 'utf8')), format: 'jwk' });
const adminEmail = need('ADMIN_EMAIL');
const mailer = { url: env.MAILER_URL ?? '', key: env.MAILER_KEY ?? '' };
const secure = publicUrl.startsWith('https:');
const whoIs = makeWhoIs({ db, tailnet, devIdentity });
const deps = {
  db, passKey, adminEmail, publicUrl, mailer, tailnet, whoIs,
  scansDir: resolve(env.SCANS_DIR ?? join(dirname(resolve(env.DB_PATH ?? 'cavecad.sqlite')), 'scans')),
};
const signin = {
  db, publicUrl, secure,
  codes: { db, secret: need('CODE_SECRET'), send: (to: string, code: string) => sendCode(mailer, { to, code }), mayGetIn: (e: string) => mayGetIn(db, e, { adminEmail, now: Date.now() }) },
  google: env.GOOGLE_CLIENT_ID ? { clientId: env.GOOGLE_CLIENT_ID, clientSecret: need('GOOGLE_CLIENT_SECRET'), redirectUri: publicUrl.replace(/\/$/, '') + '/auth/google/callback', stateSecret: need('CODE_SECRET') } : null,
};
const downloads = { db, whoIs, adminEmail, dir: env.DOWNLOADS_DIR ?? '' };
const dist = resolve(env.DIST_DIR ?? '../web/dist');

createServer((req, res) => {
  const path = (req.url ?? '/').split('?')[0];
  const p = (async () => {
    if (path === '/signin' || path.startsWith('/signin/') || path.startsWith('/auth/') || path === '/signout') {
      if (await handleSignin(signin, req, res)) return;
    }
    if (path.startsWith('/downloads/') || path === '/api/downloads') return handleDownloads(downloads, req, res);
    if (path.startsWith('/api/')) return handleApi(deps, req, res);
    return serveApp(deps, dist, req, res);
  })();
  p.catch((e) => {
    console.error(e);
    if (!res.headersSent) res.writeHead(500, { 'Content-Type': 'application/json' }).end('{"error":"server"}');
  });
}).listen(Number(env.PORT ?? 8787), env.HOST ?? '0.0.0.0', () => console.log(`CaveCAD server on ${env.HOST ?? '0.0.0.0'}:${env.PORT ?? 8787}`));
```

(`downloads.ts` arrives in Task 11; until then create it as a stub exporting
`export async function handleDownloads(_d: unknown, _req: unknown, res: import('node:http').ServerResponse) { res.writeHead(404).end(); }` so the server starts.)

- [ ] **Step 5: .env.example** — replace the identity lines (EDGE_ADDRESS, GATE_SECRET, SIGNOUT_URL, OIDC_*) with:

```
# Signs codes and Google sign-in state (32+ random bytes, hex). Make one: node -e "console.log(require('crypto').randomBytes(32).toString('hex'))"
CODE_SECRET=
# Google sign-in (optional; without it only emailed codes)
GOOGLE_CLIENT_ID=
GOOGLE_CLIENT_SECRET=
# Desktop installers + downloads.json (optional)
DOWNLOADS_DIR=/var/lib/cavecad/downloads
```

- [ ] **Step 6:** `npm test` → fail 0. Smoke: `CODE_SECRET=x PUBLIC_URL=http://localhost:8799 ADMIN_EMAIL=a@b.c PASS_KEY_FILE=<a test key> DB_PATH=/tmp/t.sqlite PORT=8799 node src/main.ts` starts and `curl -s -o /dev/null -w '%{http_code} %{redirect_url}' localhost:8799/` → `302 http://localhost:8799/signin?return=%2F`.
- [ ] **Step 7: Commit** `git add -A && git commit -m "server: run on CaveCAD's own sign-in"`

---

### Task 11: downloads.ts

**Goal:** Members download the desktop installers from cavecad.com.

**Files:**
- Create/replace: `apps/server/src/downloads.ts`
- Create: `apps/server/tests/downloads.test.ts`
- Create: `deploy/update-downloads.sh`

**Acceptance Criteria:**
- [ ] `GET /api/downloads` signed-in member → 200 manifest; signed-out → 401; no manifest → 404
- [ ] `GET /downloads/<name>` member + name listed in the manifest → 200 bytes with `Content-Disposition: attachment; filename="<name>"`; name not listed → 404; signed out → 302 `/signin?return=…`
- [ ] `deploy/update-downloads.sh` downloads `desktop-latest` assets and writes `downloads.json`

**Verify:** `node --test tests/downloads.test.ts` → all pass

**Steps:**

- [ ] **Step 1: Test:**

```ts
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { openDb } from '../src/db.ts';
import { handleDownloads } from '../src/downloads.ts';

test('downloads are for members', async () => {
  const db = openDb(':memory:');
  const dir = mkdtempSync(join(tmpdir(), 'dl-'));
  writeFileSync(join(dir, 'CaveCAD_1.0.0_universal.dmg'), 'DMG');
  writeFileSync(join(dir, 'secret.txt'), 'no');
  writeFileSync(join(dir, 'downloads.json'), JSON.stringify({ version: '1.0.0', builtAt: '2026-11-08', files: { mac: 'CaveCAD_1.0.0_universal.dmg' } }));
  const d = { db, adminEmail: 'nathan@example.com', dir, whoIs: async (r: { headers: Record<string, unknown> }) => (r.headers['x-test-user'] ? { email: String(r.headers['x-test-user']), name: 'n', provider: null, subject: '' } : null) };
  const server = createServer((req, res) => void handleDownloads(d as never, req, res));
  await new Promise<void>((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const get = (p: string, user?: string) => fetch(base + p, { headers: user ? { 'x-test-user': user } : {}, redirect: 'manual' });
  try {
    assert.equal((await get('/api/downloads')).status, 401);
    assert.equal(((await (await get('/api/downloads', 'nathan@example.com')).json()) as { version: string }).version, '1.0.0');
    const r = await get('/downloads/CaveCAD_1.0.0_universal.dmg', 'nathan@example.com');
    assert.equal(r.status, 200);
    assert.match(r.headers.get('content-disposition') ?? '', /attachment; filename="CaveCAD_1.0.0_universal.dmg"/);
    assert.equal(await r.text(), 'DMG');
    assert.equal((await get('/downloads/secret.txt', 'nathan@example.com')).status, 404);
    assert.equal((await get('/downloads/..%2Fdownloads.json', 'nathan@example.com')).status, 404);
    assert.equal((await get('/downloads/CaveCAD_1.0.0_universal.dmg')).status, 302);
    assert.equal((await get('/api/downloads', 'stranger@example.com')).status, 403);
  } finally {
    server.close();
  }
});
```

(The first admin with zero members passes `mayLoadApp`, which is what lets nathan@ in here.)

- [ ] **Step 2:** FAIL.

- [ ] **Step 3: Implement** `src/downloads.ts`:

```ts
// Desktop installers for members: the files listed in DOWNLOADS_DIR/downloads.json (written by
// deploy/update-downloads.sh), and nothing else in that folder.
import type { IncomingMessage, ServerResponse } from 'node:http';
import { createReadStream } from 'node:fs';
import { readFile, stat } from 'node:fs/promises';
import { join } from 'node:path';
import type { Db } from './db.ts';
import * as M from './members.ts';
import type { RequestLike } from './identity.ts';
import { send } from './http.ts';

export type DownloadsDeps = { db: Db; adminEmail: string; dir: string; whoIs: (r: RequestLike) => Promise<M.Identity | null> };
export type Manifest = { version: string; builtAt: string; files: Record<string, string> };

async function manifest(dir: string): Promise<Manifest | null> {
  if (!dir) return null;
  try {
    return JSON.parse(await readFile(join(dir, 'downloads.json'), 'utf8')) as Manifest;
  } catch {
    return null;
  }
}

export async function handleDownloads(d: DownloadsDeps, req: IncomingMessage, res: ServerResponse): Promise<void> {
  const path = (req.url ?? '/').split('?')[0];
  const who = await d.whoIs(req as unknown as RequestLike);
  const api = path === '/api/downloads';
  if (!who) {
    if (api) return send(res, 401, { error: 'signed-out' });
    res.writeHead(302, { Location: '/signin?return=' + encodeURIComponent(req.url ?? '/') }).end();
    return;
  }
  if (M.mayLoadApp(d.db, who, { adminEmail: d.adminEmail, now: Date.now() }) !== 'yes') return send(res, 403, { error: 'not-a-member' });
  const m = await manifest(d.dir);
  if (!m) return send(res, 404, { error: 'not-found' });
  if (api) return send(res, 200, m);
  let name: string;
  try {
    name = decodeURIComponent(path.slice('/downloads/'.length));
  } catch {
    return send(res, 404, { error: 'not-found' });
  }
  if (!Object.values(m.files).includes(name) || name.includes('/') || name.includes('..')) return send(res, 404, { error: 'not-found' });
  const file = join(d.dir, name);
  const size = (await stat(file).catch(() => null))?.size;
  if (size === undefined) return send(res, 404, { error: 'not-found' });
  res.writeHead(200, { 'Content-Type': 'application/octet-stream', 'Content-Length': size, 'Content-Disposition': `attachment; filename="${name}"`, 'Cache-Control': 'private, max-age=300' });
  createReadStream(file).pipe(res);
}
```

- [ ] **Step 4: Script** `deploy/update-downloads.sh` (chmod +x):

```bash
#!/usr/bin/env bash
# Copies the newest desktop build (GitHub release desktop-latest) to the CaveCAD server's downloads folder.
# Run on the Mac after desktop.yml succeeds: deploy/update-downloads.sh
set -euo pipefail
REPO=Nate-the-Ace/cavecad-rs
WORK=$(mktemp -d)
gh release download desktop-latest -R "$REPO" -D "$WORK" --pattern '*.dmg' --pattern '*-setup.exe' --pattern '*.msi' --pattern '*.AppImage' --pattern '*.deb' --pattern '*.rpm'
cd "$WORK"
pick() { ls $1 2>/dev/null | head -1; }
VERSION=$(pick '*.dmg' | sed -E 's/^CaveCAD_([^_]+)_.*/\1/')
cat > downloads.json <<EOF
{"version":"$VERSION","builtAt":"$(date -u +%Y-%m-%dT%H:%M:%SZ)","files":{"mac":"$(pick '*.dmg')","windows":"$(pick '*-setup.exe')","windowsMsi":"$(pick '*.msi')","linuxAppImage":"$(pick '*.AppImage')","linuxDeb":"$(pick '*.deb')","linuxRpm":"$(pick '*.rpm')"}}
EOF
COPYFILE_DISABLE=1 tar -czf /tmp/cavecad-downloads.tgz .
scp -q /tmp/cavecad-downloads.tgz root@192.168.86.10:/tmp/
ssh root@192.168.86.10 'pct push 209 /tmp/cavecad-downloads.tgz /tmp/d.tgz && rm /tmp/cavecad-downloads.tgz && pct exec 209 -- bash -c "mkdir -p /var/lib/cavecad/downloads.new && tar -xzf /tmp/d.tgz -C /var/lib/cavecad/downloads.new 2>/dev/null && rm /tmp/d.tgz && rm -rf /var/lib/cavecad/downloads.old && { [ -d /var/lib/cavecad/downloads ] && mv /var/lib/cavecad/downloads /var/lib/cavecad/downloads.old || true; } && mv /var/lib/cavecad/downloads.new /var/lib/cavecad/downloads && chown -R cavecad:cavecad /var/lib/cavecad/downloads"'
rm -rf "$WORK" /tmp/cavecad-downloads.tgz
echo "downloads: $VERSION"
```

- [ ] **Step 5:** tests pass; `npm test` fail 0. **Commit** `git add -A && git commit -m "server: desktop downloads for members"`

---

### Task 12: Web app sign-in and sign-out through /signin

**Goal:** The web app sends people to `/signin` and signs out through `/signout`; the desktop app uses the new hand-off.

**Files:**
- Modify: `apps/web/src/lib/desktop-signin.ts`, `apps/web/src/lib/account.svelte.ts`, `apps/web/src/components/account/AccountGate.svelte`, `apps/web/src/components/account/AccountSection.svelte`
- Modify: `apps/web/tests/account.check.ts` (if it covers desktop-signin's exchange/refresh)
- Modify: `.github/workflows/web.yml`, `.github/workflows/desktop.yml`

**Acceptance Criteria:**
- [ ] No `VITE_SIGNOUT_URL` left in apps/web or workflows (`grep -rn VITE_SIGNOUT_URL apps/web .github` → nothing)
- [ ] Web "Sign in" / "Sign in again" → `location.href = '/signin?return=<path>'`
- [ ] Web "Sign out" / "Use another account" → `POST /signout` then `location.href = '/signin'`
- [ ] Desktop "Sign in" opens `${VITE_SIGNIN_URL}?desktop=1&challenge=…&state=…`; the `cavecad://auth` link is exchanged at `/api/desktop-token` for `{token, expires_in}`
- [ ] AccountSection shows "with Google" or "with an emailed code"
- [ ] `cd apps/web && npm test && npx svelte-check` pass (or the project's usual type check: `npm run check` if present)

**Verify:** `cd apps/web && npm test` → no FAIL lines

**Steps:**

- [ ] **Step 1: desktop-signin.ts.** Replace the header comment, `BASE` line, `Saved`, `keep`, `exchange`, `refreshDesktop`, `startDesktopSignIn` with:

```ts
// The desktop app's sign-in: the system browser opens CaveCAD's sign-in page (Google or an emailed code) with a
// PKCE challenge; when done the page sends the browser to cavecad://auth with a one-time code, and the app swaps
// it (with its verifier) at the CaveCAD server for a 30-day session token it sends with every API call.
// In a browser none of this runs: the session cookie does the work.
import { isDesktop, openUrlNative } from './desktop.ts';

const KEY = 'cavecad-desktop-signin';
const BASE = (import.meta.env?.VITE_ACCOUNT_BASE as string | undefined) ?? '';
const SIGNIN = (import.meta.env?.VITE_SIGNIN_URL as string | undefined) ?? 'https://cavecad.com/signin';
```

```ts
type Saved = { idToken: string; until: number };
```

```ts
/** Keeps a new session token. */
export function keep(out: { token: string; expires_in: number }) {
  saved = { idToken: out.token, until: Date.now() + Math.max(60, out.expires_in - 60) * 1000 };
  try {
    localStorage.setItem(KEY, JSON.stringify(saved));
  } catch {
    // private window: the sign-in lasts for this run only
  }
}

async function exchange(body: Record<string, string>): Promise<boolean> {
  try {
    const res = await fetch(BASE + '/api/desktop-token', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) });
    if (!res.ok) return false;
    keep(await res.json());
    return true;
  } catch {
    return false;
  }
}

/** Sessions are not refreshed: true while the kept one is live, false once it ran out or the server refused it. */
export async function refreshDesktop(force = false): Promise<boolean> {
  if (force) signOutDesktop();
  return !!desktopBearer();
}

let pending: { verifier: string; state: string } | null = null;

/** Opens the system browser on CaveCAD's sign-in. The answer arrives as a cavecad://auth link. */
export async function startDesktopSignIn(): Promise<void> {
  const { verifier, challenge } = await pkcePair();
  const state = b64url(crypto.getRandomValues(new Uint8Array(16)));
  pending = { verifier, state };
  const u = new URL(SIGNIN);
  u.searchParams.set('desktop', '1');
  u.searchParams.set('challenge', challenge);
  u.searchParams.set('state', state);
  await openUrlNative(u.toString());
}
```

Delete `authorizeUrl` and its export. Leave `challengeFor`, `pkcePair`, `readAuthLink`, `desktopBearer`, `finishDesktopSignIn`, `keptSignIn`, `signOutDesktop` as they are. Fix any check in `tests/account.check.ts` that imports `authorizeUrl` or `keep({ id_token … })` to the new shapes (`keep({ token: 't', expires_in: 3600 })`).

- [ ] **Step 2: account.svelte.ts** — the sign-out function (around line 225-234): replace its last line
  `location.href = (import.meta.env.VITE_SIGNOUT_URL as string | undefined) || '/';` with:

```ts
  await fetch('/signout', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}' }).catch(() => {});
  location.href = '/signin';
```

  (make the function `async` if it is not.)

- [ ] **Step 3: AccountGate.svelte** — replace `signInAgain` and `otherAccount` and the comment above them:

```ts
  // Signing in again, or as someone else, goes through CaveCAD's sign-in page. The open cave is stored first,
  // so leaving the page cannot lose an edit made in the last moment.
  const signInAgain = async () => {
    error = '';
    account.signInError = '';
    if (isDesktop()) void startDesktopSignIn().catch(() => (error = "Couldn't open the sign-in. Check your connection and try again."));
    else {
      await flush().catch(() => {});
      location.href = '/signin?return=' + encodeURIComponent(location.pathname + location.search);
    }
  };
  const otherAccount = async () => {
    await flush().catch(() => {});
    await fetch('/signout', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{}' }).catch(() => {});
    location.href = '/signin';
  };
```

and change `<p>Sign in with your Google account.</p>` to `<p>Sign in with Google or an emailed code.</p>`.

- [ ] **Step 4: AccountSection.svelte** — replace the `by` line:

```ts
  const by = $derived(m?.provider === 'google' ? 'Google' : m?.provider === 'email' ? 'an emailed code' : m?.provider === 'microsoft' ? 'Microsoft' : '');
```

- [ ] **Step 5: Workflows** — in `.github/workflows/web.yml` delete the `VITE_SIGNOUT_URL: ${{ vars.SIGNOUT_URL }}` line. In `.github/workflows/desktop.yml` replace `VITE_SIGNOUT_URL: ${{ vars.SIGNOUT_URL }}` with `VITE_SIGNIN_URL: ${{ vars.SIGNIN_URL }}`.

- [ ] **Step 6:** `cd apps/web && npm test` passes; `npx vite build` succeeds; `grep -rn VITE_SIGNOUT_URL apps/web .github` → nothing.
- [ ] **Step 7: Commit** `git add -A apps/web .github && git commit -m "web: sign in and out through CaveCAD's own sign-in"`

---

### Task 13: Downloads panel in the web app

**Goal:** "Get the desktop app" offers the right installer first, with first-open steps.

**Files:**
- Create: `apps/web/src/lib/downloads.ts`, `apps/web/tests/downloads.check.ts`
- Create: `apps/web/src/components/account/DownloadsPanel.svelte`
- Modify: `apps/web/src/components/account/AccountSection.svelte` (button opens the panel)
- Modify: `apps/web/package.json` (add `test:downloads`, append to `test`)

**Acceptance Criteria:**
- [ ] `platformOf(ua, uaPlatform)` → `'mac'` for Macintosh/macOS, `'windows'` for Windows, `'linux'` for Linux (not Android), else `null`
- [ ] `choices(manifest, platform)` lists the person's platform first, then the others; skips missing files
- [ ] The panel is hidden inside the desktop app (`isDesktop()`), shows version, and each option shows its first-open steps
- [ ] `npm run test:downloads` → "downloads: all ok"

**Verify:** `cd apps/web && npm run test:downloads` → `downloads: all ok`

**Steps:**

- [ ] **Step 1: Check** `tests/downloads.check.ts`:

```ts
import { platformOf, choices } from '../src/lib/downloads.ts';
let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
ok(platformOf('Mozilla/5.0 (Macintosh; Intel Mac OS X 15_0) Safari', '') === 'mac', 'mac');
ok(platformOf('Mozilla/5.0 (Windows NT 10.0; Win64; x64)', 'Windows') === 'windows', 'windows');
ok(platformOf('Mozilla/5.0 (X11; Linux x86_64)', 'Linux') === 'linux', 'linux');
ok(platformOf('Mozilla/5.0 (Linux; Android 14)', 'Android') === null, 'android is not linux');
ok(platformOf('Mozilla/5.0 (iPhone; CPU iPhone OS 18_0)', 'iOS') === null, 'iphone');
const m = { version: '1.2.0', builtAt: 'x', files: { mac: 'a.dmg', windows: 'b.exe', windowsMsi: 'b.msi', linuxAppImage: 'c.AppImage', linuxDeb: 'c.deb', linuxRpm: '' } };
const w = choices(m, 'windows');
ok(w[0].platform === 'windows' && w.some((c) => c.platform === 'mac'), 'own platform first');
ok(!w.some((c) => c.file === ''), 'missing files skipped');
ok(choices(m, null)[0].platform === 'mac', 'no guess: mac first');
console.log(bad ? `downloads: ${bad} failed` : 'downloads: all ok');
process.exit(bad ? 1 : 0);
```

Add to `package.json` scripts: `"test:downloads": "node --experimental-strip-types tests/downloads.check.ts",` and append ` && npm run test:downloads` to `"test"`.

- [ ] **Step 2:** run → FAIL.

- [ ] **Step 3: Implement** `src/lib/downloads.ts`:

```ts
// The desktop downloads offered in the web app: which computer this is, and the installers in the order to show.
export type Platform = 'mac' | 'windows' | 'linux';
export type Manifest = { version: string; builtAt: string; files: Record<string, string> };
export type Choice = { platform: Platform; label: string; file: string; steps: string };

export function platformOf(ua: string, uaPlatform: string): Platform | null {
  const s = `${uaPlatform} ${ua}`;
  if (/Android|iPhone|iPad|iOS/i.test(s)) return null;
  if (/Mac/i.test(s)) return 'mac';
  if (/Win/i.test(s)) return 'windows';
  if (/Linux|X11/i.test(s)) return 'linux';
  return null;
}

const ALL: { platform: Platform; key: string; label: string; steps: string }[] = [
  { platform: 'mac', key: 'mac', label: 'Mac (Apple silicon and Intel)', steps: 'Open the downloaded file and drag CaveCAD to Applications. The first time, right-click CaveCAD, choose Open, then Open again. Your Mac asks only once.' },
  { platform: 'windows', key: 'windows', label: 'Windows installer', steps: 'Run the installer. If Windows says it protected your PC, click More info, then Run anyway.' },
  { platform: 'windows', key: 'windowsMsi', label: 'Windows (.msi, for managed PCs)', steps: 'Run the .msi. If Windows says it protected your PC, click More info, then Run anyway.' },
  { platform: 'linux', key: 'linuxAppImage', label: 'Linux AppImage', steps: 'Make the file executable (chmod +x, or Properties → Allow executing), then open it.' },
  { platform: 'linux', key: 'linuxDeb', label: 'Linux .deb (Ubuntu, Debian)', steps: 'Open it with your software installer, or: sudo apt install ./the-file.deb' },
  { platform: 'linux', key: 'linuxRpm', label: 'Linux .rpm (Fedora)', steps: 'Open it with your software installer, or: sudo dnf install ./the-file.rpm' },
];

export function choices(m: Manifest, mine: Platform | null): Choice[] {
  const list = ALL.filter((c) => m.files[c.key]).map((c) => ({ platform: c.platform, label: c.label, file: m.files[c.key], steps: c.steps }));
  const first = mine ?? 'mac';
  return [...list.filter((c) => c.platform === first), ...list.filter((c) => c.platform !== first)];
}
```

- [ ] **Step 4: Panel** `src/components/account/DownloadsPanel.svelte`:

```svelte
<script lang="ts">
  // Get the desktop app: the installer for this computer first, the others below, each with its first-open steps.
  import { choices, platformOf, type Manifest } from '../../lib/downloads';
  let { onclose }: { onclose: () => void } = $props();
  let manifest = $state<Manifest | null>(null);
  let error = $state('');
  const mine = platformOf(navigator.userAgent, (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform ?? '');
  fetch('/api/downloads').then(async (r) => (r.ok ? (manifest = await r.json()) : (error = 'The desktop app isn\'t available to download right now.'))).catch(() => (error = "Couldn't reach CaveCAD. Check your connection."));
  const list = $derived(manifest ? choices(manifest, mine) : []);
</script>

<div class="back" role="presentation" onclick={onclose}></div>
<div class="panel" role="dialog" aria-label="Get the desktop app">
  <h2>Get the desktop app</h2>
  {#if manifest}<p class="hint">Version {manifest.version}. Works offline and opens cave folders on your computer.</p>{/if}
  {#if error}<p class="hint bad">{error}</p>{/if}
  {#each list as c, i}
    <div class="item" class:first={i === 0 && c.platform === (mine ?? 'mac')}>
      <a class="btn" class:primary={i === 0} href={'/downloads/' + encodeURIComponent(c.file)} download>{c.label}</a>
      <p class="hint">{c.steps}</p>
    </div>
  {/each}
  <div class="row"><button class="btn" onclick={onclose}>Close</button></div>
</div>

<style>
  .back { position: fixed; inset: 0; background: rgb(0 0 0 / 0.3); z-index: 90; }
  .panel { position: fixed; z-index: 91; left: 50%; top: 50%; transform: translate(-50%, -50%); width: min(460px, calc(100% - 32px)); max-height: calc(100% - 32px); overflow: auto; display: flex; flex-direction: column; gap: 10px; padding: 20px; background: var(--panel); border: 1px solid var(--line); border-radius: 12px; color: var(--text); }
  h2 { margin: 0; font-size: 18px; color: var(--text-strong); }
  .item { display: flex; flex-direction: column; gap: 4px; }
  .btn { display: block; text-align: center; padding: 8px 12px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); text-decoration: none; cursor: pointer; font: inherit; }
  .btn.primary { background: var(--accent); color: #fff; border-color: var(--accent); }
  .hint { margin: 0; color: var(--muted); font-size: 12px; }
  .bad { color: var(--danger); }
  .row { display: flex; justify-content: flex-end; }
</style>
```

- [ ] **Step 5: AccountSection.svelte** — import `DownloadsPanel` and `isDesktop`:

```ts
  import DownloadsPanel from './DownloadsPanel.svelte';
  import { isDesktop } from '../../lib/desktop';
  let showDownloads = $state(false);
```

and after the Role row add:

```svelte
    {#if !isDesktop() && !account.tailnet}
      <div class="row"><span class="lab">Desktop app</span><button class="btn" onclick={() => (showDownloads = true)}>Get the desktop app</button></div>
    {/if}
```

and before the closing `{/if}` of the `{#if m}` block: `{#if showDownloads}<DownloadsPanel onclose={() => (showDownloads = false)} />{/if}`

- [ ] **Step 6:** `npm run test:downloads` and `npm test` pass; `npx vite build` succeeds.
- [ ] **Step 7: Commit** `git add -A apps/web && git commit -m "web: Get the desktop app panel"`

---

### Task 14: Deploy files and edge rules

**Goal:** The repo's deploy files describe the new setup and the edge check tests it.

**Files:**
- Modify: `deploy/Caddyfile.cave`, `deploy/check-edge.sh`, `deploy/README.md`, `deploy/tinyauth-additions.md`

**Acceptance Criteria:**
- [ ] Caddyfile.cave: cavecad.com has the header wipe and `reverse_proxy` only (no `forward_auth`, no `X-Cave-Gate`); auth.cavecad.com → `redir https://cavecad.com/signin 302`; www/old forwards unchanged
- [ ] check-edge.sh checks: signed-out `/` → 302 to `/signin`; forged `Remote-Email` on `/api/me` → 401; `/apple-touch-icon.png` → 200; `/downloads/x` signed out → 302; `api.cavecad.com/api/me` → 401
- [ ] README "Sign-in" section lists the env vars (CODE_SECRET, GOOGLE_*, MAILER_*, DOWNLOADS_DIR) and the update-downloads step; tinyauth-additions.md marked historical

**Verify:** `bash -n deploy/check-edge.sh` → no output

**Steps:**

- [ ] **Step 1:** Caddyfile.cave cavecad.com block becomes:

```
cavecad.com {
	import hardening
	route {
		request_header -Remote-User
		request_header -Remote-Email
		request_header -Remote-Name
		request_header -Remote-Groups
		request_header -Remote-Sub
		request_header -X-Cave-Gate
		reverse_proxy CAVECAD_CT:8787 {
			flush_interval -1
		}
	}
}

auth.cavecad.com {
	import hardening
	redir https://cavecad.com/signin 302
}
```

and update the header comment: "CaveCAD signs people in itself (apps/server/src/signin.ts); the edge only wipes identity headers a visitor sends (nothing believes them any more, kept as a belt) and passes everything on."

- [ ] **Step 2:** check-edge.sh body:

```bash
#!/usr/bin/env bash
# Checks the edge after deployment: signed-out visitors are sent to /signin, forged identity headers mean
# nothing, icons are public, downloads need a session, the desktop API host refuses requests without a token.
set -u
bad=0
code=$(curl -s -o /dev/null -w '%{http_code} %{redirect_url}' https://cavecad.com/)
case "$code" in 302\ https://cavecad.com/signin*) ;; *) echo "FAIL signed-out app: got $code"; bad=1 ;; esac
code=$(curl -s -o /dev/null -w '%{http_code}' -H 'Remote-Email: nathan@example.com' -H 'X-Cave-Gate: x' https://cavecad.com/api/me)
[ "$code" = 401 ] || { echo "FAIL forged header: got $code"; bad=1; }
code=$(curl -s -o /dev/null -w '%{http_code}' https://cavecad.com/apple-touch-icon.png)
[ "$code" = 200 ] || { echo "FAIL icon: got $code"; bad=1; }
code=$(curl -s -o /dev/null -w '%{http_code}' https://cavecad.com/downloads/x)
[ "$code" = 302 ] || { echo "FAIL signed-out download: got $code"; bad=1; }
code=$(curl -s -o /dev/null -w '%{http_code}' https://api.cavecad.com/api/me)
[ "$code" = 401 ] || { echo "FAIL api. without a token: got $code"; bad=1; }
[ $bad = 0 ] && echo 'edge: all ok'
exit $bad
```

- [ ] **Step 3:** README: replace the TinyAuth sign-in steps with a "Sign-in (since 2026-11)" section: env vars from `.env.example`, `CODE_SECRET` made on the container with the node one-liner and never shown, Google client redirect URI `https://cavecad.com/auth/google/callback`, mailer re-authorised for GmailApp, `deploy/update-downloads.sh` after each desktop build. Prefix tinyauth-additions.md with `> HISTORICAL (until 2026-11): CaveCAD now signs people in itself; see README "Sign-in".`
- [ ] **Step 4: Commit** `git add -A deploy && git commit -m "deploy: own sign-in, edge without forward_auth"`

---

### Task 15: End-to-end check on the dev server

**Goal:** Prove the whole sign-in works for real before cavecad.com changes.

**Files:** none (`.claude/launch.json` entry may be added for `signin-dev`)

**Acceptance Criteria:**
- [ ] Dev server (PUBLIC_URL http://localhost:8790, real MAILER_URL/KEY from the server env, CODE_SECRET random, ADMIN_EMAIL nathan's) sends a real code to support@cavecad.com, and signing in with it lands in the app
- [ ] Wrong code shows "That code isn't right. 4 tries left."
- [ ] Sign out returns to /signin and `/api/me` is 401 afterwards
- [ ] A local desktop build (`VITE_ACCOUNT_BASE=http://localhost:8790 VITE_SIGNIN_URL=http://localhost:8790/signin`) signs in through the browser and its `/api/me` works
- [ ] Google sign-in is checked at cut-over (needs the public redirect URI)

**Verify:** screenshots of the signed-in app and the desktop app's account page, plus the `/api/me` responses

**Steps:**
- [ ] Start the server with the env above (`npm run dev` in apps/server, `DIST_DIR` = a fresh `npx vite build` of apps/web) and open http://localhost:8790/ in the browser pane.
- [ ] Use support@cavecad.com as the email (an admin invites it first through the dev copy's Admin section, or set ADMIN_EMAIL=support@cavecad.com for the dev run); read the code from cavecad.app's Gmail in the browser pane.
- [ ] Record results in the plan's tasks file.

---

### Task 16: Cut-over on cavecad.com

**Goal:** cavecad.com runs CaveCAD's own sign-in; TinyAuth leaves.

**Files:** none in the repo (server and edge configuration)

**Acceptance Criteria:**
- [ ] Nathan's yes in chat before each live change (classifier rule)
- [ ] Google client "TinyAuth" has redirect URI `https://cavecad.com/auth/google/callback`
- [ ] Apps Script mailer updated (Code.combined.gs), re-authorised by Nathan for GmailApp, new deployment version; MAILER_URL unchanged
- [ ] CT 209 server.env: `CODE_SECRET` (made on the container), `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` (copied from edge /opt/edge/tinyauth-cave/google-client-secret unseen), `DOWNLOADS_DIR`; EDGE_ADDRESS/GATE_SECRET/SIGNOUT_URL/OIDC_* removed; backup `server.env.bak-before-own-signin`
- [ ] Web + server deployed (backups `*.bak-before-own-signin`, DB backed up first with cavecad-backup.service); `deploy/update-downloads.sh` run after desktop.yml
- [ ] Edge Caddyfile per Task 14 (backup `Caddyfile.bak-before-own-signin`); `deploy/check-edge.sh` → `edge: all ok`
- [ ] Nathan signs in with Google and with a code on cavecad.com; `tinyauth-cave` stopped (`docker compose stop tinyauth-cave`), folder kept a week; household TinyAuth CAVE entry removed
- [ ] Repo var `SIGNIN_URL=https://cavecad.com/signin` set; `SIGNOUT_URL` var deleted; desktop.yml run; memory notes updated

**Verify:** `bash deploy/check-edge.sh` → `edge: all ok`, and Nathan confirms both sign-ins in chat

**Steps:** as listed in the criteria, in that order; stop and ask Nathan whenever a step needs his sign-in, consent screen or yes.
