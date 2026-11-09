# Putting CaveCAD accounts online

Steps marked **(Nathan)** change the server or need his accounts: ask before each one, show the exact
change, and back up every file first (`*.bak-before-cavecad`). Read the server notes in memory first:
media-server-setup, gaming-vm-plan, feedback-inventory-before-changing-server.

1. Inventory: list every container, VM and address on the Proxmox host before choosing a new id and
   address for CaveCAD. Do not reuse one. Static addresses stay below .20 (the router's DHCP pool).
2. **(Nathan)** Create the container (unprivileged Debian 13, small: 2 cores, 2 GB, 8 GB disk), install
   Node 26, add user `cavecad`, folders /opt/cavecad/{server,web} and /etc/cavecad (root), and
   /var/lib/cavecad owned by `cavecad` (mode 700). Firewall: accept 8787 from the edge VM only (step 6).
3. Make the pass key on the container, as the user the service runs as, so the service can read the
   mode-600 file. `tools/make-pass-key.mjs` is in the repository root, not in `apps/server`: copy it over
   first, then

       runuser -u cavecad -- node /opt/cavecad/make-pass-key.mjs /var/lib/cavecad/pass-key.json

   It prints `VITE_PASS_PUBLIC_KEY=...` (public, not a secret). Put that value in the GitHub secret
   PASS_PUBLIC_KEY; the web build needs it.
4. Build the web app on GitHub (`gh workflow run web.yml`, see "Updating the web app"). Delete `dist/local`
   (real cave data, never shipped) and check it is gone before copying. Copy `apps/server` (without its
   `*.sqlite*` and `pass-key.json`) to /opt/cavecad/server and `dist` to /opt/cavecad/web.
5. **(Nathan)** Sign-in settings, then install `cavecad-server.service` and start it (next section).
6. **(Nathan)** Edge: add `Caddyfile.cave` (with the container address), DDNS names, and the ufw outbound
   rule to the container INSERTED above the LAN deny (an appended rule gives a silent 502). Reload Caddy.
   Keep the container firewall rule that accepts 8787 from the edge (192.168.86.13) only, for example
   `nft add rule inet filter input ip saddr 192.168.86.13 tcp dport 8787 accept` with a drop for the rest of
   8787. Rate limits trust the LAST X-Forwarded-For entry, which is only honest if nothing but the edge can
   reach the port.
7. Run `bash deploy/check-edge.sh` until it prints `edge: all ok`. Icons and /privacy.html are public;
   everything else signed out answers 302 to /signin.
8. **(Nathan)** Sign in at https://cavecad.com/signin (email code or Google). Then open
   https://cavecad.com/api/me in the same browser: it must show his email and `"role":"admin"`. Invite a
   second address of his own and sign in with it on another device.
9. **(Nathan)** Desktop app: build with the repository variables SIGNIN_URL=https://cavecad.com/signin and
   ACCOUNT_BASE=https://api.cavecad.com (SIGNOUT_URL is no longer used), plus PASS_PUBLIC_KEY. Offline: a
   tab already open keeps working without the network, and so does the desktop app; a new tab cannot open
   CaveCAD from nothing.

## Sign-in (since 2026-11)

CaveCAD signs people in itself: an emailed code or Google. TinyAuth, `forward_auth`, `X-Cave-Gate` and
`auth.cavecad.com` (now only a redirect to /signin) are gone. The session cookie is `__Host-cavecad_session`
on https (plain `cavecad_session` only on http://localhost in dev).

Environment in /etc/cavecad/server.env (mode 600), template `apps/server/.env.example`:

- Required: `PUBLIC_URL` (https://cavecad.com), `ADMIN_EMAIL`, `PASS_KEY_FILE`, `CODE_SECRET`, `MAILER_URL`, `MAILER_KEY` (without the mailer, emailed sign-in codes are never sent; the server logs a warning at startup).
- `CODE_SECRET` is at least 32 characters. Make it on the container and never display or paste it:
  `node -e "console.log(require('crypto').randomBytes(32).toString('hex'))"` (write the output straight into
  the env file).
- Optional: `GOOGLE_CLIENT_ID` and `GOOGLE_CLIENT_SECRET` (the existing OAuth client, named "TinyAuth" in Google
  Cloud project cavecad, needs the redirect URI `https://cavecad.com/auth/google/callback` added), `DOWNLOADS_DIR`
  (/var/lib/cavecad/downloads), `DB_PATH`, `SCANS_DIR`, `DIST_DIR`, `HOST`, `PORT`.
- Removed: `EDGE_ADDRESS`, `GATE_SECRET`, `SIGNOUT_URL`, `OIDC_ISSUER`, `OIDC_CLIENT_ID`,
  `OIDC_CLIENT_SECRET`. Delete them from the env file.

Mailer: the sign-in codes go out through the Apps Script. Open the script project, replace its code with
`tools/invite-mailer/Code.combined.gs`, re-authorise it (it now needs GmailApp and sends from
support@cavecad.com), and deploy a new version of the SAME deployment so `MAILER_URL` stays valid.

Desktop downloads: after each successful `desktop.yml` build run `deploy/update-downloads.sh` on the Mac.
It copies the release files into `DOWNLOADS_DIR` on CT 209; the site serves them to signed-in people.

## For now: on the tailnet only (2026-10-23, Nathan)

Until the public address, CaveCAD runs in CT 209 `cavecad` (192.168.86.14) and is reached only at
https://cavecad.tailbe4f4a.ts.net through `tailscale serve` (tailscaled in userspace mode, no tun device).
The server listens on 127.0.0.1 with `TAILNET_IDENTITY=1`: it believes `Tailscale-User-Login`, which serve
sets and strips from visitors. (Historical: superseded by the public setup above.) Only tailnet members the Tailscale ACL lets reach `cavecad` can use it.
Going public: see "Sign-in (since 2026-11)". Nathan chose (2026-10-23): keep
play/games on their own login (no Google there), and TinyAuth sessions of 30 days.

## Updating the web app (2026-10-24)

Build it on GitHub so the settings kept in its secrets go in (the live relay, feedback and class-link
mailboxes; PASS_PUBLIC_KEY is a repository variable):
`gh workflow run web.yml`, then `gh run download <run> -n cavecad-web -D <empty folder>`. On CT 209: back up
/opt/cavecad/web (`cp -a web web.bak-before-<what>`), empty it, unpack the build there. No restart needed.
Only the desktop workflow (`desktop.yml`) uses SIGNIN_URL and ACCOUNT_BASE; `web.yml` needs neither.

## Server caves (2026-10-26)

Caves are now stored readable on the server (spec `docs/superpowers/specs/2026-10-26-server-caves-design.md`),
so the database needs a nightly backup. On CT 209:

1. `apt install sqlite3` (the backup script uses the `sqlite3` command; it is not installed by default).
2. Install the backup unit: copy `cavecad-backup.sh` to `/usr/local/bin/` (mode 755), and
   `cavecad-backup.service` and `cavecad-backup.timer` to `/etc/systemd/system/`, then
   `systemctl daemon-reload && systemctl enable --now cavecad-backup.timer`. Test once with
   `systemctl start cavecad-backup.service`; a file `cavecad-<date>.sqlite` appears in
   `/var/lib/cavecad/backups/` (the last 14 are kept). It runs daily at 02:30, before the host's nightly
   container backup, and uses SQLite's `.backup`, which is safe while the server is running (WAL).
3. Update the server (`/opt/cavecad/server`, restart `cavecad-server`) and the web app (see "Updating the
   web app" above) as usual.

### Updating the invite mailer

The invite email text changed ("CaveCAD stores caves on its server and syncs them to your devices.").
The Apps Script is not updated by the server deploy: open the script project in Nathan's cavecad.app
account, replace its code with the new `tools/invite-mailer/Code.combined.gs`, and redeploy the web app
version (keep the same deployment so the URL stays).
