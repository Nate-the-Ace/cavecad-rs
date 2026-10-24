# Putting CaveCAD accounts online

Steps marked **(Nathan)** change the server or need his accounts: ask before each one, show the exact
change, and back up every file first (`*.bak-before-cavecad`). Read the server notes in memory first:
media-server-setup, gaming-vm-plan, feedback-inventory-before-changing-server.

1. Inventory: list every container, VM and address on the Proxmox host before choosing a new id and
   address for CaveCAD. Do not reuse one. Static addresses stay below .20 (the router's DHCP pool).
2. **(Nathan)** Create the container (unprivileged Debian 13, small: 2 cores, 2 GB, 8 GB disk), install
   Node 26, add user `cavecad`, folders /opt/cavecad/{server,web} and /etc/cavecad (root), and
   /var/lib/cavecad owned by `cavecad` (mode 700). Firewall: accept 8787 from the edge VM only.
3. Make the pass key on the container, as the user the service runs as, so the service can read the
   mode-600 file. `tools/make-pass-key.mjs` is in the repository root, not in `apps/server`: copy it over
   first, then

       runuser -u cavecad -- node /opt/cavecad/make-pass-key.mjs /var/lib/cavecad/pass-key.json

   It prints `VITE_PASS_PUBLIC_KEY=...` (public, not a secret). Put that value in the GitHub secret
   PASS_PUBLIC_KEY; the web build in the next step needs it.
4. Build the web app on the Mac, from `apps/web`, with the key from step 3:

       VITE_PASS_PUBLIC_KEY=... VITE_SIGNOUT_URL=https://auth.schonegg.stream/logout npx vite build

   The build stops if either is missing. Delete `dist/local` (real cave data from `public/local`, never
   shipped) and check it is gone before copying. Copy `apps/server` (without its `*.sqlite*` and
   `pass-key.json`) to /opt/cavecad/server and `dist` to /opt/cavecad/web. After step 9, check that
   VITE_SIGNOUT_URL really ends the TinyAuth session (sign out, reload, you must see TinyAuth's sign-in);
   if not, rebuild with TinyAuth v5's logout address.
5. **(Nathan)** Write /etc/cavecad/server.env (mode 600) from `apps/server/.env.example` (ADMIN_EMAIL = his
   Google address, EDGE_ADDRESS = the edge VM), install `cavecad-server.service`, start it. Generate one
   long random secret (`openssl rand -base64 48`) and put it in two places with the identical value:
   `GATE_SECRET` in /etc/cavecad/server.env, and `CAVE_GATE_SECRET` in the edge's environment (the Caddy
   container's compose environment), so Caddy can set `X-Cave-Gate` from it. Never set `DEV_IDENTITY` on
   the server: it signs in everyone as one email, and the server refuses to start with it beside
   `EDGE_ADDRESS` or `OIDC_ISSUER`.
6. **(Nathan)** Invite mailer: follow tools/invite-mailer/README.md; put MAILER_URL and MAILER_KEY in the env.
7. **(Nathan)** Google OAuth client under cavecad.app (he accepts the terms). Microsoft stays off at launch
   (see `tinyauth-additions.md`).
8. **(Nathan)** TinyAuth additions (`tinyauth-additions.md`), then restart TinyAuth.
9. **(Nathan)** Edge: add `Caddyfile.cave` (with the container address), DDNS names `cave` and `cave-api`,
   ufw outbound rule to the container INSERTED above the LAN deny (an appended rule gives a silent 502).
   Before reloading, check the order with `caddy adapt --pretty` in the Caddy container: in the `cave.`
   route the handler that deletes `Remote-*` must come before the forward_auth `reverse_proxy` to
   127.0.0.1:3000 (each site is one `route`, so the written order holds). Reload Caddy.
10. Run `bash deploy/check-edge.sh` until it prints `edge: all ok`.
11. **(Nathan)** Sign in. Then open https://cave.schonegg.stream/api/me in the same browser: it must show
    his email and `"role":"admin"`. A `signed-out` answer means the edge dropped TinyAuth's headers (look at
    step 9's order first). Invite a second address of his own, open the email on another device, sign in
    with it. Offline: a browser cannot open CaveCAD from nothing without the network (there is no service
    worker, and the page is never cached). What works underground is a tab that was already open when the
    network went, and the desktop app. Test that: sign in on the tablet, keep the tab open, switch to
    airplane mode, keep editing a cave, close the cave and open it again from the Cave Shelf.
12. **(Nathan)** Desktop app: build with VITE_ACCOUNT_BASE=https://cave-api.schonegg.stream and the same
    VITE_PASS_PUBLIC_KEY and VITE_SIGNOUT_URL as the web build (the build stops without them; the GitHub
    desktop workflow needs them in its environment too), sign in through the browser round trip. The app
    learns TinyAuth's sign-in address from the server (`/api/desktop-config`), so it needs no issuer setting.

## For now: on the tailnet only (2026-10-23, Nathan)

Until the public address, CaveCAD runs in CT 209 `cavecad` (192.168.86.14) and is reached only at
https://cavecad.tailbe4f4a.ts.net through `tailscale serve` (tailscaled in userspace mode, no tun device).
The server listens on 127.0.0.1 with `TAILNET_IDENTITY=1`: it believes `Tailscale-User-Login`, which serve
sets and strips from visitors. Steps 6 to 12 wait; the web build has `VITE_SIGNOUT_URL=/` (signing out
means nothing on the tailnet). Only tailnet members the Tailscale ACL lets reach `cavecad` can use it.
Going public: rebuild with the TinyAuth sign-out address, set HOST=0.0.0.0, EDGE_ADDRESS, GATE_SECRET and
drop TAILNET_IDENTITY in /etc/cavecad/server.env, then steps 6 to 12. Nathan chose (2026-10-23): keep
play/games on their own login (no Google there), and TinyAuth sessions of 30 days.

## Updating the web app (2026-10-24)

Build it on GitHub so the settings kept in its secrets go in (the live relay, feedback and class-link
mailboxes; the account values are repository variables PASS_PUBLIC_KEY, SIGNOUT_URL, ACCOUNT_BASE):
`gh workflow run web.yml`, then `gh run download <run> -n cavecad-web -D <empty folder>`. On CT 209: back up
/opt/cavecad/web (`cp -a web web.bak-before-<what>`), empty it, unpack the build there. No restart needed.
The desktop workflow (`desktop.yml`) takes the same values.
