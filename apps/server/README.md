# CaveCAD server

One container that serves the CaveCAD web app and the live-editing relay (`/sync`), and keeps every
room's operation log on disk so a room survives restarts and upgrades. It is the "relay that is not
the laptop" from `docs/DESIGN.md`: the same protocol as the dev relay, from a small always-on server.

    docker compose -f apps/server/docker-compose.yml up --build
    # open http://<this machine>:8080 on every device
    # View > Live editing > same room name > Join

## How it works

- `server.mjs` serves `apps/web/dist` (same cache rules as `apps/web/public/_headers`), `/healthz`,
  and attaches the relay from `apps/web/scripts/sync-relay.mjs` unchanged, so the web app and the
  server cannot drift apart on the protocol.
- `room-store.mjs` is the relay's optional store: one append-only JSON-lines file per room in
  `DATA_DIR`, named by a hash of the room name (a room name can never become a path). A torn last
  line from a crash is ignored. The server never reads an op, and never logs cave data.
- Merging still happens on the devices (ops converge in any order), so the server holds no
  authoritative cave state: its log is just what late joiners and reconnecting devices catch up from.

## Settings

| Variable | Default | Meaning |
| --- | --- | --- |
| `PORT` / `HOST` | `8080` / `0.0.0.0` | where to listen |
| `DATA_DIR` | `./data` (`/data` in the image) | room logs; mount a volume here |
| `STATIC_DIR` | `../web/dist` | the built web app |

Run without Docker: build the web app (`cd apps/web && npm run build`), then
`cd apps/server && npm install && npm start`. Tests: `npm test` here.

## Know before exposing it

- **No sign-in.** Anyone who can reach the port and knows a room name can join and edit it. Keep it
  on a LAN or VPN, or put an authenticating reverse proxy (with WebSocket support for `/sync`) in
  front. Real permissions are a later co-editing phase.
- **The data volume holds the caves.** Room logs contain whatever the cave holds, entrance
  coordinates included. Treat the volume like the caves themselves: restrict access, back it up,
  and do not share it.
- **Logs only grow** (trimming is not built yet) and a room's log is held in memory while the
  server runs.
