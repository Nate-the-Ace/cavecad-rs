# Putting CaveCAD accounts online

Steps marked **(Nathan)** change the server or need his accounts: ask before each one, show the exact
change, and back up every file first (`*.bak-before-cavecad`). Read the server notes in memory first:
media-server-setup, gaming-vm-plan, feedback-inventory-before-changing-server.

1. Inventory: list every container, VM and address on the Proxmox host before choosing a new id and
   address for CaveCAD. Do not reuse one.
2. **(Nathan)** Create the container (unprivileged Debian 13, small: 2 cores, 2 GB, 8 GB disk), install
   Node 26, add user `cavecad`, folders /opt/cavecad/{server,web}, /var/lib/cavecad, /etc/cavecad.
   Firewall: accept 8787 from the edge VM only.
3. Copy `apps/server` to /opt/cavecad/server and the web build (`npx vite build` with the production
   VITE_PASS_PUBLIC_KEY, VITE_SIGNOUT_URL=https://auth.schonegg.stream/logout) to /opt/cavecad/web.
   The build stops if either is missing. Check that VITE_SIGNOUT_URL really ends the TinyAuth session
   (sign out, reload, you must see TinyAuth's sign-in); adjust to TinyAuth v5's logout address if not.
4. Make the pass key on the container: `node tools/make-pass-key.mjs /var/lib/cavecad/pass-key.json`;
   put the printed value in the GitHub secret PASS_PUBLIC_KEY and the build.
5. **(Nathan)** Write /etc/cavecad/server.env (mode 600) from `apps/server/.env.example` (ADMIN_EMAIL = his
   Google address), install `cavecad-server.service`, start it. Generate one long random secret
   (`openssl rand -base64 48`) and put it in two places with the identical value: `GATE_SECRET` in
   /etc/cavecad/server.env, and `CAVE_GATE_SECRET` in the edge's environment (the Caddy container's
   compose environment), so Caddy can set `X-Cave-Gate` from it. Never set `DEV_IDENTITY` on the server:
   it signs in everyone as one email, and the server refuses to start with it beside `EDGE_ADDRESS` or
   `OIDC_ISSUER`.
6. **(Nathan)** Invite mailer: follow tools/invite-mailer/README.md; put MAILER_URL and MAILER_KEY in the env.
7. **(Nathan)** Google OAuth client and Microsoft app registration under cavecad.app (he accepts their terms).
8. **(Nathan)** TinyAuth additions (`tinyauth-additions.md`), then restart TinyAuth.
9. **(Nathan)** Edge: add `Caddyfile.cave` (with the container address), DDNS names `cave` and `cave-api`,
   ufw outbound rule to the container INSERTED above the LAN deny (an appended rule gives a silent 502).
   Reload Caddy.
10. Run `bash deploy/check-edge.sh` until it prints `edge: all ok`.
11. **(Nathan)** Sign in, invite a second address of his own, open the email on another device, sign in
    with it, then sign in on a tablet, switch to airplane mode and reopen CaveCAD.
12. **(Nathan)** Desktop app: build with VITE_ACCOUNT_BASE=https://cave-api.schonegg.stream and the same
    VITE_PASS_PUBLIC_KEY and VITE_SIGNOUT_URL as the web build (the build stops without them; the GitHub
    desktop workflow needs them in its environment too), sign in through the browser round trip. The app
    learns TinyAuth's sign-in address from the server (`/api/desktop-config`), so it needs no issuer setting.
