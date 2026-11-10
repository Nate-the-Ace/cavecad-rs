# Server caves, steps 1-3 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers-extended-cc:subagent-driven-development (recommended) or superpowers-extended-cc:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Caves are stored readable on the CaveCAD server and synced to each member's devices with no key steps; the caves locked during rooms step 1 move over by themselves; Share, Live, Classroom and Join are gone; every screen and email says truthfully where caves live.

**Architecture:** Build steps 1-3 of `docs/superpowers/specs/2026-10-26-server-caves-design.md`. The rooms step 1 transport stays (numbered entries, catch-up then live stream, pending queue in IndexedDB, watchdog, stop guards, chunking). A per-cave `storage` flag (`locked` | `open`) lets the server accept plain JSON text for open caves; new caves are open; a device holding a locked cave's key converts it once. The client's lock/unlock/key paths survive only for that conversion. PR #6's Cave Shelf listing ("On your other devices") is kept and switched to open labels and snapshots.

**Tech Stack:** Node 26 server (`node:sqlite`, `node:test`), Svelte 5 web app, WebCrypto only for migration, Apps Script (invite email text).

**User decisions (already made):**
- "Yes, confirmed: store caves fully readable on the server, synced to my devices, and retire Share, Live, Classroom and Join."
- "The server is the home, and each cave can also keep a linked folder copy" — folder copy is step 5, not in this plan.
- "Yes: a class becomes a room with watcher seats, the driver seat and raise a hand, rebuilt in the later phases."
- "Yes: keep the open edit history plus snapshots, reusing the rooms connection."

**Scope:** steps 1-3 only. Step 4 (rooms, seats, links, waiting room without keys) and step 5 (linked folder copy, Bring in changes) get their own plans.

**Conventions for every task:**
- Work in `~/Documents/github/cavecad-rs-caves` (branch `server-caves`). Never touch other worktrees.
- Read the current files before editing; this plan names behaviour and exact files, and the code it changes was written recently (rooms step 1 and PR #6), so match its existing shapes and names.
- Server tests: `cd apps/server && node --test`. Web: `cd apps/web && npm test && npm run check`; build: `npx vite build` (needs `apps/web/.env.local` with `VITE_PASS_PUBLIC_KEY` and `VITE_SIGNOUT_URL`; copy it from `~/Documents/github/cavecad-rs-rooms/apps/web/.env.local` if missing; never commit it).
- Node strip-only TypeScript: no parameter properties or enums.
- Entrance coordinates may now be stored on the server, but never in logs, URLs or console output.
- Commit after each task; messages end with a blank line and `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

---

### Task 1: Open storage on the server

**Goal:** The server stores open caves' entries, snapshots and labels as plain JSON text, keeps locked caves working until converted, and converts a locked cave to open when its owner's device sends an open snapshot.

**Files:**
- Modify: `apps/server/src/db.ts`, `apps/server/src/rooms.ts`, `apps/server/src/rooms-api.ts`
- Test: `apps/server/tests/rooms.test.ts`, `apps/server/tests/rooms-api.test.ts`

**Acceptance Criteria:**
- [ ] `caves` has `storage TEXT NOT NULL DEFAULT 'locked' CHECK (storage IN ('locked','open'))`; an existing database gains the column on start (an `ALTER TABLE ... ADD COLUMN` guarded by a `PRAGMA table_info` check, since `CREATE TABLE IF NOT EXISTS` won't add it).
- [ ] `POST /api/caves {id, storage: 'open'}` makes an open cave with no wrapped keys; `wrapped` is optional for open caves and refused (`bad-request`) if given with `storage: 'open'`.
- [ ] For an open cave, `POST entries`, `PUT snapshot` and `PUT label` accept any JSON text up to the existing size caps (no base64 check); the stored text and the stream line carry it unchanged; `version` is not required (store 0).
- [ ] For a locked cave, all existing behaviour and checks are unchanged.
- [ ] `POST /api/caves/:id/open {upTo, snapshot}` (owner only): requires `upTo` equal to the cave's highest entry number; in one transaction stores the open snapshot at `upTo`, deletes the cave's locked history rows, locked snapshot, wrapped keys and locked label, sets `storage = 'open'`. Entry numbering continues after `upTo`. A second call on an open cave is `bad-request`.
- [ ] `GET /api/caves` returns `storage` per cave; for open caves `wrapped` is null and `needsKeys` false.
- [ ] Device registration and the device header stay (they identify devices for presence and pending queues later).

**Verify:** `cd apps/server && node --test` → `# fail 0`

**Steps:**
- [ ] Write failing tests in `rooms.test.ts` (open cave create/append/snapshot/label with plain text containing `{"ops":"..."}` and an entrance-like string; locked cave unchanged; conversion: locked cave with 3 entries → `openCave(…, upTo: 3, snapshot)` → history empty, snapshot open at 3, next append gets number 4, wrapped keys gone; conversion refused with wrong `upTo` and on an open cave; non-owner refused) and in `rooms-api.test.ts` (the same over HTTP plus `GET /api/caves` showing `storage`).
- [ ] Implement in `rooms.ts`: a `storage(db, caveId)` helper; branch validation in the API on it (base64 regex only for `locked`); `createCave(db, memberId, caveId, wrapped, now, storage = 'locked')`; `openCave(db, memberId, caveId, upTo, snapshot, now)`.
- [ ] Run tests, commit: "Server: open cave storage and converting a locked cave to open".

---

### Task 2: The sync client in open mode, and converting locked caves

**Goal:** `room-client.ts` syncs open caves with no keys (plain `{c, ops}` entries, plain `{upTo, ops}` snapshots, plain labels), creates every new room open, and converts a locked cave it holds the key for.

**Files:**
- Modify: `apps/web/src/lib/room-client.ts`, `apps/web/src/lib/room.svelte.ts`, `apps/web/src/lib/server-caves.svelte.ts`
- Test: `apps/web/tests/roomclient.check.ts`

**Acceptance Criteria:**
- [ ] New rooms are created with `storage: 'open'`; no cave key is made, no wrapping.
- [ ] For `storage: 'open'`: entries posted as the JSON text `{"c": clientId, "ops": ops}`; received entries parsed the same way (own client id skipped); snapshots `{"upTo": n, "ops": ...}` with inner `upTo` matching; labels as plain JSON `{name, summary}` (match PR #6's label content); no `waiting-key` state.
- [ ] For `storage: 'locked'` with this device holding the key: after catching up (unlocking as today), the client calls `POST /api/caves/:id/open` with `upTo` = last number and the current open snapshot, then switches itself to open mode for the rest of the session. If another device converted first (`bad-request` because the cave is already open), it reloads `/api/caves` and continues in open mode.
- [ ] For `storage: 'locked'` without the key: unchanged behaviour (`waiting-key`) — this only matters until the owner's keyed device opens the cave once.
- [ ] Pending entries saved before conversion in the locked format are re-posted after conversion as open entries (unlock them with the key held in memory before dropping it; if the key is gone, keep them and report `error` with a clear reason rather than dropping them).
- [ ] The shelf's server-cave listing (`server-caves.svelte.ts`, PR #6) reads plain labels and plain snapshots for open caves; opening an open cave from "On your other devices" needs no key and works on a device that never had one.
- [ ] All existing roomclient checks still pass (adapted where they assumed keys for new rooms); new checks: two devices on an open cave edit both ways; a third device with no keys opens the open cave from the listing; a locked cave made with the old path converts when its keyed device opens it, entry numbers continue, and a second device then syncs it with no key; pending locked entries survive conversion.

**Verify:** `cd apps/web && npm run test:roomclient` → `roomclient: all ok`; `npm test` exit 0

**Steps:**
- [ ] Read `room-client.ts` fully (it carries stop guards, pending, chunking, watchdog, labels from PR #6) and `server-caves.svelte.ts`. Add a `mode: 'open' | 'locked'` the client sets from `/api/caves` (`storage`).
- [ ] Write the new failing checks first, then implement. Keep the crypto imports only on the locked/migration path.
- [ ] Run, commit: "Room client: open caves need no keys; locked caves convert when their keyed device opens them".

---

### Task 3: Retire Share, Live, Classroom and Join

**Goal:** The peer-to-peer, relay and classroom features and their settings are gone from the app, the dev server and the build; old live links land on a plain "moved" page.

**Files (delete unless another kept module imports from them; check with `grep -rn` before each delete):**
- `apps/web/src/lib/`: `p2p.svelte.ts`, `p2p-code.ts`, `p2p-ice.ts`, `p2p-relay.svelte.ts`, `p2p-wire.ts`, `classroom.svelte.ts`, `classroom-log.ts`, `classroom-mirror.ts`, `classroom-room.mjs`, `classroom-room.d.mts`, `signal.ts`, `signal-handshake.ts`, `share-base.ts`, and the live-session parts of `sync.svelte.ts` (keep `me`, `rename`, `COLORS`, `hasSavedName` and anything the account or presence code imports — move them to a new `lib/me.svelte.ts` if `sync.svelte.ts` becomes empty otherwise).
- `apps/web/src/components/`: `LiveShare.svelte`, `ClassroomAlerts.svelte`, `ClassroomDialogs.svelte`, `ClassroomDock.svelte`, `MailboxSettings.svelte`, `RelaySettings.svelte`, `CollaborateGroup.svelte` if it only holds Share/Join/Live/Classroom buttons.
- `apps/web/scripts/`: `sync-relay.mjs`, `classroom-relay.mjs`, `vite-plugin-cave-sync.mjs` (and its use in `vite.config.ts`).
- `apps/web/tests/`: `p2p.check.ts`, `relay.check.mjs`, `classroom.check.ts` (keep its readonly-classification part by moving that check into a new `tests/readonly.check.ts`), `signal.check.ts`; remove their scripts from `package.json`; drop the `ws` dependency if nothing else uses it.
- `tools/signal-mailbox/` (whole folder), `tools/live-test.sh`.
- Modify: `App.svelte`, `Ribbon.svelte` / the Home tab's Live group, `CaveShelf.svelte` ("Join a live session…"), `UserMenu.svelte` (People tab live-room parts), `readonly.ts` (remove the class lock source, keep the account lock), `room.svelte.ts` (remove the "close the room while a session is active" logic), `desktop.ts`/`App.svelte` deep-link handling (`cavecad://join` → the moved page), `.github/workflows/pages.yml`/`web.yml`/`desktop.yml` (drop `VITE_RELAY_URL`, `VITE_SIGNAL_*` env lines).
- Create: a small `MovedPage` (in `AccountGate.svelte` or its own component) shown when the address has `?live=`, `?class=` or `?join=`: "Live sessions moved to rooms. Open your caves from the Cave Shelf." with a button that clears the query and opens the shelf.

**Acceptance Criteria:**
- [ ] `grep -rniE "p2p|classroom|LiveShare|signal-handshake|sync-relay|RelaySettings|MailboxSettings" apps/web/src apps/web/scripts apps/web/tests` finds nothing except the moved-page code and `readonly` comments.
- [ ] No Share, Join, Live or Classroom button remains in the ribbon, shelf, user window or menus.
- [ ] Opening `/?live=abc`, `/?class=abc` or `/?join=room&invite=x` shows the moved page; `cavecad://join?...` in the desktop app shows it too.
- [ ] Presence drawing code (`presence-draw.ts`, pointer rendering) is kept if anything else uses it; if only the deleted modules used it, keep the file with a note "used by room presence (rooms step 4)" rather than deleting it.
- [ ] `npm test`, `npm run check` (0 errors), `npx vite build` all pass; `cargo test --workspace` unaffected.

**Verify:** `cd apps/web && npm test && npm run check && npx vite build` → exit 0, `0 ERRORS`

**Steps:**
- [ ] Map imports first: `grep -rn "from './p2p\|from '../lib/p2p\|classroom\|sync.svelte\|signal\|share-base" apps/web/src` and list every importer; decide per file (delete, trim, or move kept exports).
- [ ] Delete and trim in small commits if helpful; finish with one commit "Retire Share, Live, Classroom and Join; old live links land on a moved page".
- [ ] Update `docs/DESIGN.md` with a dated line recording the retirement and why (Nathan, 2026-10-26).

---

### Task 4: Truthful wording everywhere

**Goal:** Every place that promised the server can't read caves now says where caves live and who can read them.

**Files:**
- Modify: `apps/web/src/components/account/AccountGate.svelte` (welcome note), `apps/web/public/privacy.html` (or wherever the privacy page lives; find it with `grep -rln "privacy" apps/web/public apps/server`), `tools/invite-mailer/Logic.gs` and `Code.combined.gs` (email body), `tools/invite-mailer/test.mjs`, `CLAUDE.md` (entrance coordinate rule), `docs/superpowers/specs/2026-10-23-accounts-design.md` and `2026-10-25-server-rooms-phase1-design.md` (a "Superseded 2026-10-26" note at the top of the affected sections, pointing to the server caves spec).

**Acceptance Criteria:**
- [ ] Welcome note, titled "Where your caves live", text exactly: "Your caves are stored on CaveCAD's server and synced to your devices so you can work offline. Only people you give a seat in a cave's room can open it. The server's admins can technically read everything stored on it."
- [ ] The privacy page says the same in its own words, including that entrance coordinates are stored on the server, plus the existing account facts; no sentence anywhere still claims the server cannot read caves (`grep -rniE "can't read|cannot read|can't open it|locked history" apps/web docs tools` finds only historical spec text marked superseded).
- [ ] Invite email body: replace the "never saves your cave data ... in a form it can read" sentence with "CaveCAD stores caves on its server and syncs them to your devices."; `node tools/invite-mailer/test.mjs` passes.
- [ ] `CLAUDE.md` rule becomes: "Never log entrance coordinates or put them in URLs or files meant for sharing (exports, packages). The server stores caves, coordinates included (Nathan, 2026-10-26); `cave-log` redaction stays as a safety net. The per-cave `<cave>.log` is excluded from sanitized packages."

**Verify:** `node tools/invite-mailer/test.mjs` → `invite-mailer: all ok`; `cd apps/web && npm test` exit 0; the grep above returns only superseded spec text.

**Steps:**
- [ ] Make the edits; run the checks; commit "Wording: caves live on the server (welcome note, privacy page, invite email, CLAUDE.md rule)".
- [ ] Note for the deploy: the Apps Script invite mailer must be updated by pasting the new `Code.combined.gs` into the script project (Nathan's account) — record this in `deploy/README.md` "Updating the invite mailer".

---

### Task 5: Server backups and deploy notes

**Goal:** The server database is backed up nightly in a WAL-safe way, and the deploy notes say how to ship this plan.

**Files:**
- Create: `deploy/cavecad-backup.sh`, `deploy/cavecad-backup.service`, `deploy/cavecad-backup.timer`
- Modify: `deploy/README.md`

**Acceptance Criteria:**
- [ ] `cavecad-backup.sh` runs `sqlite3 /var/lib/cavecad/cavecad.sqlite ".backup '/var/lib/cavecad/backups/cavecad-$(date +%F).sqlite'"`, keeps the last 14 files, exits non-zero on failure, and is shellcheck-clean (`bash -n` at least).
- [ ] The timer runs daily at 02:30 local time, before the host's nightly container backup.
- [ ] `deploy/README.md` gains "Server caves (2026-10-26)": install the backup unit, then the usual server + web update; note that `sqlite3` must be installed on CT 209 (`apt install sqlite3`), and the invite-mailer paste step from Task 4.

**Verify:** `bash -n deploy/cavecad-backup.sh` → no output

**Steps:**
- [ ] Write the three files and the README section; commit "Deploy: nightly WAL-safe backup of the cave database; server caves deploy notes".

---

### Task 6: Check in a browser

**Goal:** The real app syncs an open cave between a browser and a scripted second device through a local server, converts an old locked cave, and shows no retired controls.

**Files:**
- Modify: `~/Documents/Claude/.claude/launch.json` (add `caves-server` on :8789 and `caves-web` on :5186 for this worktree, `CAVECAD_API_PORT=8789`, `PUBLIC_URL=http://localhost:5186`)

**Acceptance Criteria:**
- [ ] A new cave reaches "Synced"; the server's `history` and `snapshots` rows for it are readable JSON (sqlite3 query shows the trip name).
- [ ] A scripted second device (no keys) lists the cave and receives a live trip rename.
- [ ] `/?live=x` shows the moved page; no Share/Join/Live/Classroom controls on screen.
- [ ] The welcome note shows the new wording.

**Verify:** screenshots plus the sqlite3 query output.

**Steps:**
- [ ] Start the two launch entries, sign in (dev identity), create a cave, check, script the second device the same way the rooms step 1 check did (`/private/tmp/.../scratchpad/rooms/device2.ts`, adapted: no keys), record results in `docs/DESIGN.md` under a dated "Server caves steps 1-3" section, commit.

---

## Not in this plan

Rooms, seats, hosts, links, waiting room without keys (step 4); linked folder copy and Bring in changes (step 5); deleting `room-crypto.ts`/key code entirely (after every locked cave is converted — a later clean-up, tracked in DESIGN.md).
