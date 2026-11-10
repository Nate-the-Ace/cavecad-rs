# Emailed-code sign-in, CaveCAD's own sign-in, and desktop downloads

Date: 2026-11-08. Status: approved by Nathan in chat (product choices below), spec awaiting his review.

## Why

Invites already go to any email address, but signing in needs a Google account, so someone invited at a
Yahoo or work address cannot get in. Nathan chose an emailed code. He also wants it on the desktop app from
day one, and wants the web app to offer desktop downloads.

## Product decisions (Nathan, 2026-11-08)

- Sign-in screen offers **Continue with Google** and **Email me a code**.
- Codes work for the website **and** the desktop app from day one.
- The web app offers desktop downloads for Mac, Windows and Linux.
- The desktop app stays **unsigned** (no Apple/Microsoft fees); each download shows short first-open steps.

Decided by me (Nathan asked for product questions only):

- One account per email address, whichever way someone signs in.
- A code is 6 digits, lives 10 minutes, allows 5 tries, works once.
- Codes are sent only to an email that may get in (member, live invite, the admin, or anyone while
  sign-ups are open). Every request gets the same answer ("If that address can use CaveCAD, a code is on its
  way"), so the form neither reveals who is a member nor mails strangers.
- Signed in for 30 days, as now. The offline pass (pass.ts) is unchanged.
- Downloads are for signed-in members only; CaveCAD stays invite only.

## Approach

Three were weighed:

1. Add codes beside TinyAuth: the CaveCAD server keeps its own sessions for code users, and the edge asks the
   server, which asks TinyAuth. Two sign-in systems stitched together.
2. Put an off-the-shelf identity server that does email codes behind TinyAuth. Another service to run and
   update for one feature.
3. **Chosen:** the CaveCAD server does all of sign-in itself, Google (directly, OAuth 2 with PKCE) and codes.
   TinyAuth leaves cavecad.com (container `tinyauth-cave` retires). This is the "own sign-in at public
   launch" step identity.ts was written for: only the identity adapter and the sign-in pages change; members,
   invites, rooms and caves do not.

## Server units (apps/server/src)

Each file has one job and its own test file.

### sessions.ts

- Table `sessions(id, token_hash, member_email, kind 'web'|'desktop', created_at, expires_at, last_seen_at,
  revoked_at)`. The token is 32 random bytes (base64url); only its SHA-256 is stored.
- `create(email, kind, now)` returns the token; life 30 days. `find(token, now)` returns the email or null
  (expired or revoked is null). `revoke(token)`, `revokeAll(email)` (used when a member is removed).
- Web: cookie `cavecad_session`, `HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age=2592000`.
- Desktop: the same token sent as `Authorization: Bearer`.

### email-code.ts

- Table `email_codes(email, code_hmac, expires_at, tries_left, used_at, created_at, ip)`.
- `request(email, ip, now)`: normalises the email, checks it may get in (new `members.mayGetIn`, beside `mayLoadApp`: member not removed, live invite, the admin email, or sign-ups open), checks
  limits, makes a 6-digit code with `crypto.randomInt`, stores `HMAC-SHA256(CODE_SECRET, email + code)`,
  sends it. Always answers `{ ok: true }`. A new code replaces any earlier unused one for that email.
- Limits: one code per email per 60 s, 5 per email per hour, 20 requests per IP per hour. Over a limit:
  nothing is sent, the answer is unchanged.
- `verify(email, code, now)`: wrong code takes a try; at 0 tries or past expiry the code is dead and the
  answer is "That code has expired. Ask for a new one." A right code marks it used and returns the email.
  Compare with `timingSafeEqual`.
- `CODE_SECRET`: 32 random bytes in server.env, made on the container, never shown.

### google.ts

- `start(returnTo, desktop?)`: makes state + PKCE verifier, keeps them in a short-lived signed cookie
  `cavecad_oauth` (10 min), redirects to Google with scope `openid email profile`.
- `callback(code, state)`: checks state, swaps the code at Google's token endpoint (client secret stays on
  the server), verifies the ID token against Google's JWKS (`iss` accounts.google.com, `aud` our client id,
  `exp`, `email_verified === true`). The JWT check reuses identity.ts's verifier, moved to `jwt.ts`.
- Client: the existing Google client "TinyAuth" (project `cavecad`); add redirect URI
  `https://cavecad.com/auth/google/callback`. Client id + secret go to server.env (`GOOGLE_CLIENT_ID`,
  `GOOGLE_CLIENT_SECRET`), the secret copied from the edge's file unseen.

### signin.ts (pages and routes)

- `GET /signin`: a small self-contained HTML page (no app bundle, so strangers never load the app): the
  Google button, an email box, then a 6-digit box. Light/dark like the app. Keeps `?return=` and the desktop
  hand-off parameters.
- `POST /signin/code` `{email}` → always `{ok:true}`. `POST /signin/verify` `{email, code}` → sets the
  session cookie (or finishes the desktop hand-off) and answers where to go next.
- `GET /auth/google/callback` → same ending.
- `POST /signout` (and `GET /signout` with a confirm page): revokes the session, clears the cookie,
  shows `/signin`.
- Who gets in after sign-in is unchanged: `members.resolve` (invites, sign-ups, admin) decides. A signed-in
  stranger still gets the "invite only" page.

### identity.ts

`whoIs` believes, in order: a desktop bearer session token, the `cavecad_session` cookie, the tailnet and
dev identities (kept for local work). The Remote-* header path and the TinyAuth OIDC path are deleted, and
with them `EDGE_ADDRESS`, `GATE_SECRET`, `OIDC_*` and `SIGNOUT_URL`. `Identity.provider` becomes
`'google' | 'email' | null`, stored on the member as now.

### Desktop hand-off (replaces desktop-token.ts)

1. The app makes a PKCE pair and a state and opens the system browser at
   `https://cavecad.com/signin?desktop=1&challenge=…&state=…`.
2. The person signs in there (Google or code). The server makes a one-time hand-off code (60 s life,
   stored hashed with the challenge) and redirects to `cavecad://auth?code=…&state=…`. The page also shows
   "You can go back to CaveCAD now".
3. The app posts `{code, verifier}` to `https://api.cavecad.com/api/desktop-token`; the server checks the
   verifier against the challenge and answers `{token, expires_in}`: a 30-day `desktop` session.
4. No refresh tokens: after 30 days the app asks to sign in again (the offline pass already covers time
   underground). `desktop-signin.ts` keeps its PKCE and link-reading code; `/api/desktop-config` goes.

### Mail

`mailer.ts` gains `sendCode(to, code)`; the Apps Script (tools/invite-mailer) gains a `code` kind and sends
both invites and codes **from support@cavecad.com** (`GmailApp` alias, which now exists) with a plain
subject "Your CaveCAD sign-in code: 123456". The script keeps its KEY check and per-hour cap. Consumer Gmail
allows about 100 recipients a day; if a send fails the page says "We couldn't send the email just now. Try
again in a few minutes." and the server logs it.

## Desktop downloads

- Files live on CT 209 in `/var/lib/cavecad/downloads/` with `downloads.json`:
  `{version, builtAt, files: {mac, windows, windowsMsi, linuxAppImage, linuxDeb, linuxRpm}}`.
- `GET /downloads/<file>`: members only (session required), `Content-Disposition: attachment`.
  `GET /api/downloads` returns the manifest.
- `deploy/update-downloads.sh`: fetches the `desktop-latest` release assets with `gh`, writes the manifest,
  pushes both to CT 209. Run after each desktop build (part of "republish").
- Web: **Get the desktop app** in the account menu opens a panel: the computer's own platform first
  (from `navigator.userAgentData`/`userAgent`), the others below, version shown. Under each button, the
  first-open steps:
  - Mac: "Open the downloaded file and drag CaveCAD to Applications. The first time, right-click CaveCAD,
    choose Open, then Open again. Mac asks only once."
  - Windows: "Run the installer. If Windows says it protected your PC, click More info, then Run anyway."
  - Linux: AppImage ("make it executable, then open it"), .deb and .rpm.
  Hidden inside the desktop app itself.

## Edge and cut-over

- cavecad.com: no `forward_auth`; keep the identity-header wipe. api.cavecad.com unchanged.
  auth.cavecad.com redirects to `https://cavecad.com/signin`. Old schonegg.stream redirects stay.
- `tinyauth-cave` stops after the switch is verified; its folder is kept for a week, then removed.
  The household TinyAuth's stale CAVE app entry is removed in the same edit.
- Everyone signs in once more (new cookie). Members are matched by email, so nothing else changes.
- The desktop app needs the new build (made by desktop.yml) and one sign-in.

## Errors (what people see)

- Wrong code: "That code isn't right. N tries left."
- Expired or out of tries: "That code has expired. Ask for a new one."
- Google says the email isn't verified: "Google hasn't confirmed that email. Use Email me a code instead."
- Google sign-in cancelled or failed: back to `/signin` with "Google sign-in didn't finish. Try again."
- Desktop hand-off code expired: the app shows "Sign-in took too long. Try again."

## Testing

- Unit tests (node --test) per file: sessions (create/find/expiry/revoke/revokeAll), email-code (generic
  answer for strangers, limits, tries, expiry, single use, replacement, HMAC not plain code in the table),
  google (state mismatch, bad signature, wrong aud/iss, unverified email, success), desktop hand-off
  (verifier mismatch, reuse, 60 s expiry), identity (cookie, bearer, revoked, header spoofing now ignored),
  downloads (member only, manifest), api routes end to end with fake mailer and fake Google.
- Web: the downloads panel picks the right platform; the sign-in flow against a local server.
- Before cut-over: a full run on the dev server (launch.json) with a real code to support@ and a real Google
  sign-in, then the desktop hand-off with a local desktop build. deploy/check-edge.sh is rewritten for the
  new rules (no session → /signin redirect; forged Remote-* ignored; downloads need a session).

## Not in this work

- Microsoft sign-in, passkeys, a paid mail service, Apple/Windows code signing, auto-update in the desktop
  app.
