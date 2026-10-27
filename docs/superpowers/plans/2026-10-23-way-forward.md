# CaveCAD accounts and server: the way forward

Date: 2026-10-23. Where things stand, and the order of the work still to do. Each stage ends in
something Nathan can try. Steps marked **(Nathan)** need his accounts, his consent screens or his
go-ahead on a server change; everything else an agent can do and show him.

## Where it stands

- Branch `accounts` (unmerged, unpushed; last commits c11c6da review fixes, 8a8fb3c tailnet identity):
  required invite-only accounts, the CaveCAD server (`apps/server`), Account / Drives / Admin in the user
  window, the welcome page with "Your caves stay yours", the offline pass, desktop sign-in code, the
  Pages redirect, deploy files.
- Running privately: CT 209 `cavecad` (192.168.86.14) behind `tailscale serve` at
  https://cavecad.tailbe4f4a.ts.net. Identity comes from Tailscale (`TAILNET_IDENTITY=1`), not TinyAuth.
  Nathan is signed in as admin. The edge server and TinyAuth are untouched.
- Only Nathan can reach it: the Tailscale rules let other people reach only the media VM.
- Nathan reports the sign-in landing page "is not functioning right now". Stage 0 finds out what.

## Stage 0: make the private copy work properly (next session, first)

1. Ask Nathan what he sees fail (which page, which button), then read `journalctl -u cavecad-server`
   on CT 209 and the browser console on the tailnet address.
2. Likely suspects, check in this order:
   - `VITE_SIGNOUT_URL=/` in the tailnet build: Sign out and "Use another account" only reload, and
     under Tailscale identity there is no other account to choose. Hide both while on the tailnet.
   - The welcome page's buttons need `PATCH /api/me`. The CSRF rule refuses writes whose `Origin` is
     not `PUBLIC_URL` exactly (`https://cavecad.tailbe4f4a.ts.net`). Check the value and the scheme.
   - "Connect a drive" only records a choice (connectors are piece 2) and Reconnect does nothing: say so
     on the screen ("coming soon") so it does not look broken.
   - Invites: no mailer is set up, and invitees cannot reach the tailnet, so inviting is not testable yet.
3. Done when: Nathan can open the tailnet address, pass the welcome page, change his name and colour,
   record and disconnect a drive, open Admin, and the screens say plainly what is not built yet.

## Stage 1: go public at cave.schonegg.stream (piece 4)

Order matters; each server step is shown to Nathan before it is made, with a backup first.

1. **(Nathan)** Google sign-in: create an OAuth client (Web application) in Google Cloud under
   cavecad.app@gmail.com, redirect URI `https://auth.schonegg.stream/api/oauth/callback/google`. He types
   the client id and secret into TinyAuth's config himself.
2. TinyAuth (v5.2.0 facts from its source, in memory): add the Google provider; for the cave app set the
   OAuth whitelist to allow everyone (`/.*/`, CaveCAD decides who gets in); keep play. and games. on
   their own login (Nathan's choice); session expiry 30 days. Microsoft stays off (unverified email).
3. Edge Caddy: add `deploy/Caddyfile.cave` (the `route {}` version), DDNS names `cave` and `cave-api`,
   ufw outbound rule to 192.168.86.14:8787 INSERTED above the LAN deny. Run `caddy adapt` on the edge
   before reloading.
4. CT 209: switch `/etc/cavecad/server.env` from `TAILNET_IDENTITY=1` to `EDGE_ADDRESS=192.168.86.13`
   plus a new `GATE_SECRET` (the edge gets the same value as `CAVE_GATE_SECRET`); `PUBLIC_URL` and the
   web build's `VITE_SIGNOUT_URL` become the public ones (`https://auth.schonegg.stream/logout`).
   Decide whether to keep the tailnet address as a second way in (needs both identity paths at once; not
   built) or retire it. Recommendation: retire it, one way in.
5. Run `deploy/check-edge.sh` until `edge: all ok`, plus the real-session check (signed-in browser
   reaches `/api/me` as a member).
6. **(Nathan)** Sign in with Google at https://cave.schonegg.stream. Done when he is admin there.

## Stage 2: invites that send themselves

1. **(Nathan)** Set up the invite mailer Apps Script under cavecad.app (`tools/invite-mailer/README.md`):
   he approves the Gmail permission; the key goes into the server's env.
2. **(Nathan)** Invite a second address of his own, open the email on another device, sign in.
3. Then one or two real cavers. Done when an invited caver signs in and sees the welcome page.

## Stage 3: merge to main and retire the old Pages app

1. Before merging: tell anyone who used the old GitHub Pages copy to export their shelf (the redirect
   page now offers it) because their caves are stored in that browser only.
2. Merge `accounts` into `main`. The Pages workflow then publishes the redirect page.
3. Add `apps/server` tests and the web tests to CI (they run locally only today).
4. Done when main is the accounts code and the Pages address forwards to the public site.

## Stage 4: the desktop app signs in

1. TinyAuth OIDC client `cavecad-desktop` (redirect `cavecad://auth`), server env `OIDC_*`, the
   `cave-api.schonegg.stream` site.
2. Check whether TinyAuth issues refresh tokens; if not, desktop users go through the browser each time
   their sign-in runs out (acceptable, note it).
3. GitHub secrets for `desktop.yml`: `VITE_PASS_PUBLIC_KEY`, `VITE_SIGNOUT_URL`, `VITE_ACCOUNT_BASE`.
4. **(Nathan)** Sign in from the desktop app, then open it in airplane mode. Done when both work.

## Stage 5: drives (piece 2), Google Drive first

1. Spike first: under Google's narrow `drive.file` permission, does picking a folder in the Google Picker
   give access to the files inside it? The answer decides the whole Drive design. Needs Drive enabled
   on the same Google Cloud project.
   **Answered 2026-10-24 (Nathan ran the test page):** no. With `drive.file`, a folder picked in the
   Google Picker is visible to the app but its files and subfolders are not (`'<folder>' in parents`
   lists nothing, so nothing inside can be read). Set up for the test in Google Cloud project `cavecad`:
   Drive API and Picker API enabled, `drive.file` added under Data Access (non-sensitive), web client
   "CaveCAD Drive (web)" (origins https://cave.schonegg.stream and http://localhost:5199), browser API key
   "CaveCAD Picker (browser)" limited to the Picker API and those two sites. So the design must choose:
   picking files (Picker multi-select) under `drive.file`, a broader restricted scope (`drive.readonly`
   or `drive`: Google's warning screen and a 100-user cap until a yearly security review), or the desktop
   app's synced-folder route only. Nathan decides.
2. Then the Drive connector in the browser (keys stay on the device), Reconnect and "N caves come from
   it" made real, caves on the Cave Shelf from Drive.
3. Then OneDrive, Dropbox, WebDAV, each its own small spec. Each needs a developer app under cavecad.app
   that Nathan creates.

## Stage 6: live sessions through the server (piece 3)

Replace the Apps Script mailbox and the public Metered relay key with the server: the existing relay
moves into `apps/server`, edits are locked end to end (key in the link's `#` part), the server keeps
the locked edit history so people can join a cave whose host is offline. Needs its own spec first.

## Stage 7: public launch (when Nathan is happy with the product)

- CaveCAD's own Google sign-in replaces TinyAuth (only `identity.ts` changes).
- Microsoft sign-in, with a verified email.
- Google brand verification for the Drive permission; Dropbox production approval.
- Invite and notice email beyond Gmail's daily limit.
- Decide on offline use in a browser (a service worker that only falls back to the cache when offline;
  today only an open tab or the desktop app works underground).
- Load and upload check on the home connection.
- Flip Admin > Sign-ups to "Open to anyone".

## Decisions still Nathan's

- Stage 1 step 4: retire the tailnet address once public? (Recommended: yes.)
- Stage 3: anyone besides Nathan with caves in the old Pages copy?
- Stage 7: when to launch publicly.
