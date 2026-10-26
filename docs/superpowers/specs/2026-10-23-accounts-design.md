# Accounts: sign-in, members, invites and connected drives

Date: 2026-10-23. Status: design approved by Nathan, not built.

This is piece 1 of 4. The others build on it and get their own specs:

1. **Accounts** (this spec): who you are, the member list, invites, the record of your connected drives.
2. Drive connectors: Google Drive, OneDrive, Dropbox, WebDAV.
3. Server-hub live sessions: the relay with a locked (end-to-end encrypted) edit history.
4. Public hosting at `cave.schonegg.stream` behind TinyAuth.

Pieces 1 and 4 ship together (accounts need the server to exist), but the account code does not
depend on how the server is reached.

## Decisions (Nathan, 2026-10-23)

- **Sign-in is required.** Nobody uses CaveCAD without an account. A device that has signed in keeps
  working offline (see "Offline pass").
- **Invite only for now.** One switch (`Invite only` / `Open to anyone`) opens sign-ups at public
  launch; nothing else has to change.
- **Sign in with an existing Google or Microsoft account.** No CaveCAD passwords.
- **Only admins invite.** Nathan is the first admin and can make others admins. Members cannot invite.
- **TinyAuth does the sign-in now; CaveCAD keeps the member list.** At public launch CaveCAD gets its
  own Google/Microsoft sign-in and TinyAuth drops out. Only the identity adapter changes.
- **Drives:** a member can connect any number (two Google accounts, Dropbox, OneDrive, WebDAV).
  Drive keys stay in the browser on each device; the server only records which drives a member uses.
  Disconnect is everywhere, never per device.
> **Superseded 2026-10-26:** caves are now stored on the server readable by it, and synced to devices; see `2026-10-26-server-caves-design.md`. The locked-history text below is historical.
- **The server never holds cave data in a form it can read.** It may keep a locked edit history for
  live sessions (piece 3). The key exists only on the devices of the people sharing the cave and in
  the invite link's `#` fragment, which browsers never send to a server. Admins cannot read it either.
  The first-sign-in screen says so (wording below). Any later feature that would break this sentence
  is out of bounds.

## What the person sees

All of it lives in the window that opens from the user icon at the top right of the tab bar (today:
People, Options, Feedback). The window gets a side list of sections:

- **Account**: initials circle, name, "Signed in with Google · email", Sign out; display name and
  colour (the ones live sessions show); role.
- **Drives**: one row per connected drive (provider icon, provider name, account email). State per
  row: `Connected here` (badge) or `Reconnect` (button) when this device has no key yet. Every row
  has `Disconnect`. Below the list: `Connect a drive`.
  - Disconnect asks inline: "Disconnect Dropbox (email)? CaveCAD stops using it on every device. Your
    caves stay in your Dropbox, untouched. N caves on your shelf come from it." Buttons `Keep it` and
    `Disconnect`.
  - After disconnect, caves from that drive leave the Cave Shelf until it is reconnected. A cave from
    it that is open stays open and asks where to save (another connected drive or this device).
- **People, Options, Feedback**: unchanged.
- **Admin** (admins only):
  - `Sign-ups`: Invite only / Open to anyone.
  - `Invite someone`: email, role (Member/Admin), `Send invite`. Help line: the link works once, for
    that email only, for 7 days; Copy link is there to text it instead.
  - `Waiting to join`: email, role, sent, expires; `Copy link`, `Resend`, `Cancel`.
  - `Members (n)`: initials, name, role, provider, last seen, invited by; role dropdown and `Remove`
    for everyone but yourself. The last admin cannot be removed or demoted.

### Sign-up path

1. Email from cavecad.app@gmail.com: "Nathan invited you to CaveCAD. Sign in with rick@example.com.
   Link works for 7 days." Button `Join CaveCAD`.
2. TinyAuth's sign-in page: `Continue with Google`, `Continue with Microsoft`.
3. First-sign-in screen (once only; afterwards the app opens on the Cave Shelf):
   - > **Superseded 2026-10-26:** the note is now titled "Where your caves live" and says the server stores caves readable by its admins; see `2026-10-26-server-caves-design.md`.
   - Note, under a lock icon, titled **Your caves stay yours**:
     > Your caves live on your devices and in the drives you connect. CaveCAD's server never has your
     > surveys, sketches, scans or locations in a form it can read.
     >
     > For live sessions, the server keeps a locked history of edits so people can catch up. Only the
     > people you share a cave with hold the key. The server, and its admins, can't open it.
     >
     > The server knows your name, email, which drives you use, and when edits happened.
   - `Name shown to others` (prefilled from the sign-in).
   - `Where are your caves?`: `Connect a drive` or `Keep on this device for now`.
4. Signed in with an email that has no account and no invite: "CaveCAD is invite only for now. You're
   signed in as (email), which hasn't been invited. If you were invited at another address, sign in
   with that one." Button `Use another account`. When sign-ups are open, this page becomes step 3.

An invite is bound to its email: a forwarded link does nothing for another address.

## How it works

### Server (`apps/server`, new)

One Node program on its own Proxmox container. It serves the built web app, the account API, and (in
piece 3) the live-session relay. It is the only thing behind `cave.schonegg.stream`.

**Storage**: one SQLite file, covered by the host's nightly backup. Tables:

| Table | Columns |
|---|---|
| `members` | id, email (unique, lower-case), display_name, colour, role (`admin`/`member`), provider (`google`/`microsoft`), invited_by, created_at, last_seen_at, removed_at |
| `invites` | id, email, role, token_hash (SHA-256 of the link token; the token itself is never stored), invited_by, created_at, expires_at, used_at, cancelled_at |
| `drives` | id, member_id, provider (`gdrive`/`onedrive`/`dropbox`/`webdav`), account_label (email or WebDAV address), created_at |
| `settings` | key, value (`signups` = `invite`/`open`) |

Nothing else. No caves, scans, coordinates, or drive keys. Server clocks are fine here: this is
server bookkeeping, not cave edits, so the "never wall-clock time" rule for operations does not apply.

**Identity adapter**: the only code that decides who a request is. One function,
`whoIs(request) -> { email, name, provider, subject } | null`.

- Now: reads `Remote-Email`, `Remote-Name`, `Remote-Sub` set by TinyAuth through Caddy, and accepts
  them only from the edge server's address. TinyAuth's documented headers do not say which provider
  (Google or Microsoft) was used: check during the build whether `Remote-Sub` or a TinyAuth claim
  reveals it; if not, `provider` is null under TinyAuth and the page leaves it out.
- Desktop and other cross-site callers: a bearer token from TinyAuth's OIDC provider, verified against
  its public key (same adapter, second branch).
- At public launch: replaced by CaveCAD's own Google/Microsoft OIDC sign-in and session cookie. The
  rest of the server is unchanged.

**Membership check** on every API request: `whoIs`, then look up `members` by email. No member and a
valid invite token in the request: create the member, mark the invite used. No member, no invite,
`signups = invite`: answer `not-invited`. `removed_at` set: answer `removed`.

**API** (JSON over HTTPS, all behind the membership check):

- `GET /api/me`: the member, their drives list, and an offline pass.
- `PATCH /api/me`: display name, colour.
- `POST /api/me/drives`, `DELETE /api/me/drives/:id`: record or forget a connected drive.
- Admin only: `GET /api/admin/members`, `PATCH /api/admin/members/:id` (role), `DELETE
  /api/admin/members/:id` (remove), `GET/POST /api/admin/invites`, `POST /api/admin/invites/:id/resend`,
  `DELETE /api/admin/invites/:id`, `PUT /api/admin/settings/signups`.
- Rules enforced by the server, not just the page: only admins call admin routes; the last admin
  cannot be demoted or removed; a member cannot change their own role.

**Invite email**: sent through an Apps Script under cavecad.app@gmail.com, the same pattern as the
Send Feedback receiver (consumer Gmail allows about 100 a day, enough while invite only). The server
holds the script's key. If sending fails, the invite still exists and the admin sees `Copy link`.

### Edge (Caddy on the edge VM)

- `cave.schonegg.stream` block: `forward_auth` to TinyAuth, `copy_headers Remote-User Remote-Email
  Remote-Name Remote-Sub`, proxy to the CaveCAD container.
- **Strip every `Remote-*` header the visitor sent before `forward_auth`** (one `request_header
  -Remote-User` line per header: User, Email, Name, Groups, Sub).
  Without this, a request that TinyAuth answers without those headers would carry whatever the visitor
  typed, and the server would believe it.
- TinyAuth's Google and Microsoft providers allow any account: TinyAuth only proves who someone is;
  CaveCAD decides who gets in.
- The CaveCAD container's firewall accepts only the edge VM. DDNS name added; the edge's outbound ufw
  rule is inserted above the LAN deny (see the server notes: an appended rule gives a silent 502).
- TinyAuth `auth.sessionExpiry` raised from the default 24 h to 30 days.

### Web app

- `lib/account.svelte.ts`: signed-in member, drives list, role, sign-up state; talks to `/api/me` and
  the admin routes; holds the offline pass.
- Window sections: `AccountSection.svelte`, `DrivesSection.svelte`, `AdminSection.svelte`, added to the
  user-icon window beside People, Options, Feedback. Admin is not rendered for members.
- `WelcomePage.svelte` (first sign-in) and `NotInvitedPage.svelte`.
- The app starts by asking `/api/me`. Member: normal start. First visit: welcome page. `not-invited` /
  `removed`: those pages. No network: offline pass (below).
- Drive connecting itself is piece 2. In this piece `Connect a drive` lists the providers and records
  the choice; the row shows `Reconnect` until piece 2 supplies the key.

### Offline pass

`/api/me` returns a pass signed by the server (Ed25519): member id, email, role, issued, expires
(30 days). The device keeps it in IndexedDB. With no network the app checks the signature with the
server's public key (shipped in the build) and the expiry, and opens on what is stored on the device.
With network, `/api/me` is asked again and a new pass replaces the old one. A removed member's pass
stops working the next time their device is online. An expired pass with no network: the app opens
read-only with "Connect once to keep editing". Underground use is the reason this exists.

### Desktop app

Signs in through TinyAuth's OIDC provider (`oidc.clients.cavecad-desktop`): the app opens the system
browser, the person chooses Google or Microsoft, TinyAuth redirects to `cavecad://auth?...` (the scheme
is already registered), and the app exchanges the code (PKCE) for a token it sends as a bearer token.
Same offline pass as the web app.

### GitHub Pages copy

Becomes a page that sends people to `cave.schonegg.stream`. The build no longer ships the app there.

## Errors

- Server unreachable while online-looking: treated as offline (pass rules), with a quiet notice.
- Invite link expired, used, cancelled, or for another email: the not-invited page, with the reason
  ("This invite has expired. Ask (inviter) for a new one.").
- Session expired at TinyAuth while the app is open: the next API call fails with a redirect; the app
  says "Your sign-in expired" and offers `Sign in again` (reloads through TinyAuth). Unsaved edits are
  kept on the device first.
- Email send failure: invite kept, admin told, `Copy link` offered.
- Removing yourself, or demoting or removing the last admin: refused with a reason, by the server.

## Testing

- Server unit tests (node, temporary SQLite): membership check for every case (member, invite valid,
  invite expired / used / cancelled / wrong email, sign-ups open, removed); admin rules (non-admin
  refused, last admin protected, self role change refused); invite token stored only as a hash.
- Identity adapter: headers accepted only from the edge address; a request with `Remote-Email` from
  anywhere else is ignored; bearer token signature and expiry checked.
- Offline pass: signature verifies, tampered pass refused, expired pass refused, read-only fallback.
- Web: account store and sections against a fake API (same pattern as `tests/signal.check.ts`).
- Edge: after deployment, `curl` with a forged `Remote-Email` and a real browser User-Agent must get
  the TinyAuth login, not the app (TinyAuth decides browser vs 401 by User-Agent).
- By a person: real Google and Microsoft sign-ins, a real invite email, the desktop sign-in round
  trip, and a device opened in airplane mode after signing in.

## Before public launch (not this piece)

- Replace TinyAuth with CaveCAD's own sign-in (identity adapter only).
- Google brand verification for the Drive connection; Dropbox production approval (needed within two
  weeks of 50 linked users).
- Invite and notification email beyond Gmail's daily limit.
- Check home upload speed and server load for many users.

## Out of scope

Drive connectors (piece 2), live sessions and the locked edit history (piece 3), hosting setup steps
(piece 4), CaveCAD passwords, member-sent invites, per-device drive disconnect, a signed-in devices
list.

## As built

Where the build differs from the spec above:

- Members have a `welcomed_at` column, so the first-sign-in welcome shows once. A removed member who is re-invited has it cleared and sees the welcome again.
- The invite link parameter is `?signup=`, not `?invite=` (relay rooms already use `invite`). The web app keeps the token in `sessionStorage` until the sign-in succeeds, then forgets it.
- A live invite is accepted by matching the signed-in email even when the link token is missing. The token is only used to explain why a link failed (used, expired, cancelled).
- The gate header `X-Cave-Gate` carries a shared secret (`GATE_SECRET` on the server, `CAVE_GATE_SECRET` on the edge), compared in constant time. Identity headers are believed only from `EDGE_ADDRESS`.
- The desktop app uses a second host, `cave-api.schonegg.stream`, with no `forward_auth` and a bearer ID token. `/api/desktop-token` exchanges the sign-in code on the server (keeps the client secret off the desktop). `/api/desktop-config` gives the app TinyAuth's sign-in address, so the app never calls TinyAuth itself.
- Writes need `Content-Type: application/json` and a same-origin request (CSRF), except the desktop app with a bearer and the desktop token exchange.
- Bearer tokens: RS256 or EdDSA, each tied to its key type; `email_verified` must not be false; 60 seconds of clock leeway.
- Microsoft sign-in stays off at launch until its email claim is one Microsoft verifies (nOAuth); Google only.
- The server refuses to start with `DEV_IDENTITY` beside `EDGE_ADDRESS` or `OIDC_ISSUER`, or with a non-localhost `PUBLIC_URL`.
- Offline: an expired pass makes the cave read-only through a separate account lock (the classroom has its own). A pass that fails its signature, or a build with no key, signs the device out.
- The web and desktop builds fail without `VITE_PASS_PUBLIC_KEY` and `VITE_SIGNOUT_URL`. `.github/workflows/desktop.yml` does not set either yet; it needs both from secrets before it builds this branch.
- The last-admin guard is a backstop: `self` plus `not-admin` already make it unreachable through the API.
- Edge: each Caddy site is one `route { }`, so the header wipe runs before `forward_auth` (Caddy's own order would run `forward_auth` first and the wipe would delete TinyAuth's headers).
- The old GitHub Pages address sends people on, but first offers the caves its Cave Shelf kept in that browser as DXF files (that storage belongs to the old address, so the new site cannot see it).
- Until the public address the server can run on the tailnet only (`TAILNET_IDENTITY=1`, behind `tailscale serve` on its own container, listening on 127.0.0.1): `Tailscale-User-Login` is the email.
- TinyAuth sends only printable ASCII in its headers (`SanitizeHeader`), so a name like José arrives as Jos; the person can fix it in Account.
- The app itself is withheld (Nathan, 2026-10-24): the server serves the web app only to a member, someone with an invite waiting for their email, or anyone while sign-ups are open (`mayLoadApp`). Anyone else who signs in with Google gets a short "invite only" page with no app code. The privacy page stays open to all.

## Not built

- **A browser cannot open CaveCAD from nothing without the network.** There is no service worker and `index.html` is never cached (the refresh-is-enough rule in CLAUDE.md). The offline pass works in a tab that was already open when the network went, and in the desktop app, which carries its own copy of the web app. Underground, use one of those. An app-shell service worker would fix it but conflicts with refresh-always-fresh; build it only if Nathan asks.
