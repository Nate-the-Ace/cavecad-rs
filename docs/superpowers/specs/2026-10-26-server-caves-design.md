# Server caves: caves live on the CaveCAD server

Date: 2026-10-26. Status: design approved by Nathan, not built.

This replaces the end-to-end locked design of `2026-10-25-server-rooms-phase1-design.md` for storage and
sharing. The rooms, seats, hosts, links, waiting room and "Everyone with access" decisions of that spec still
apply, without any key steps. The seven-phase feature list in that spec still applies; classes come back as a
room in the later phases.

## Decisions (Nathan, 2026-10-26)

- **Caves are stored on the server, readable by it** (entrance coordinates included) and synced to the
  member's devices, which keep a copy for offline work. Reason, in his words: "the key exchange is really
  clunky and not intuitive ... we should start storing caves data on the server itself and just make sure it
  syncs locally." Confirmed twice after the privacy cost was laid out (options offered: fully open, open
  except entrance coordinates, locked with a passkey).
- **Retire now:** Share, Join, Live (peer-to-peer and relay sessions), Classroom, the Google Apps Script
  signalling mailbox, the Metered relay. Only the server/client scheme remains.
- **The server is each cave's home.** A cave can also keep a **linked folder copy** (for example inside a
  Google Drive folder) that CaveCAD keeps up to date **one way**: server cave to folder. Changes made to files
  in the folder come in only when the person presses **Bring in changes**.
- **Classes return later as a room**: watcher seats, the driver seat, raise a hand, the class record
  (phases 2, 3 and 5).
- **Storage approach:** the open edit history plus snapshots, reusing the rooms step 1 connection.
- Unchanged: sign-in required, invite only, admins approve newcomers, owners and hosts run rooms, keep-a-copy
  setting per room, you see only your own room, owners see everyone with access.

## Approaches considered

1. **Open edit history plus snapshots (chosen).** Reuses the numbered entries, catch-up, live stream, offline
   queue and watchdog from rooms step 1; entries are no longer locked. Keeps who-changed-what for the activity
   feed, replay and restore (phase 4), and lets the server read caves for previews, PDFs and search (phase 7).
2. Latest whole cave only: simplest, no history, concurrent edits overwrite each other.
3. Server runs the cave engine and validates every edit: most capable, too big now; approach 1 grows into it
   in phase 7.

## What people see

- **Cave Shelf** lists caves from the server: the member's own and those shared with them, with a "3 here"
  badge when people are in a cave's rooms. The device keeps a copy so the shelf and caves open offline.
- **New cave, Import file, Open folder** create a server cave. **Export** still writes DXF, Compass, Walls,
  Survex, Therion and the CaveCAD file.
- **Linked folder copy** (per cave, optional): "Keep a copy in a folder…" picks a folder. CaveCAD writes the
  CaveCAD file, a DXF and the survey data files there a few seconds after the cave goes quiet. **Bring in
  changes** compares the folder's files with the cave, shows the differences, and brings in only what the
  person accepts; where the cave and a file disagree on the same item, the person picks which wins. Works in
  the desktop app and in Chromium browsers (the folder picker); elsewhere the option is hidden.
  If the folder cannot be written (permission lost, disk full), the cave keeps working and a note says
  "Folder copy paused".
- **Rooms window:** as in the phase 1 spec (Private room, extra rooms, seats, hosts, links, waiting room,
  Everyone with access, Revoke all), with no key steps. The "Waiting for your other device" state is gone.
- **Retired:** Share, Join, Live, Classroom buttons, the live-session join page, invite and reply codes, the
  mailbox and relay settings. An old live link (`?live=`, `?class=`, `?join=`) shows "Live sessions moved to
  rooms" with a link to the Cave Shelf.
- **Wording** (replaces "Your caves stay yours" on the first-sign-in page; the privacy page and invite email
  say the same):
  > Your caves are stored on CaveCAD's server and synced to your devices so you can work offline. Only people
  > you give a seat in a cave's room can open it. The server's admins can technically read everything stored
  > on it.

## How it works

- **Server:** history entries and snapshots are stored as plain JSON text (the ops and the snapshot), still
  numbered per cave and sent to every open stream of the cave. Seats are enforced on the server: watchers'
  entries are refused, removed members are refused at once and their open streams closed, links expire. No
  key records: the `devices` and `wrapped_keys` tables and routes are removed once migration is done.
- **Client:** `room-client.ts` keeps the connection, catch-up, offline pending queue (IndexedDB), watchdog,
  backoff, chunking and the stop guards; lock/unlock, wrapping, key memory and `waiting-key` are removed.
  `room-crypto.ts` and `device-key.ts` (keys) are deleted after migration.
- **Migration of the caves locked during rooms step 1:** the server marks them `locked`. When a device that
  holds the key opens one, it unlocks the history once, uploads an open snapshot, and the server switches the
  cave to open storage and deletes the locked rows. Until then the cave opens from the device's copy.
- **Folder copy:** written by the device (desktop app native file access, or the browser's directory handle
  remembered in IndexedDB), never read back automatically. Bring in changes reads the files, imports them
  into a scratch cave in the engine, diffs against the open cave by stable ids and shots, and applies the
  accepted changes as ordinary edits.
- **Backups:** the server database is backed up nightly with `sqlite3 .backup` (WAL-safe) into the host's
  existing backup job, since caves now live there.
- **Retirement:** remove the Share/Join/Live/Classroom UI and their modules (`p2p*.ts`, `sync.svelte.ts` live
  parts, `classroom*.ts`, `signal*.ts`, `sync-relay.mjs`, `classroom-relay.mjs`, the mailbox settings) and
  their tests; keep anything the room presence will reuse (pointer and ghost drawing) by moving it into room
  code. Build variables `VITE_RELAY_URL`, `VITE_SIGNAL_*` and the GitHub secrets for them become unused.

## When things go wrong

- **Offline:** work on the device copy; pending edits post on reconnect.
- **Removed from a cave:** the copy stays on the device but gets no new edits; watchers' copies are wiped if
  the room's setting says watchers don't keep one.
- **Folder copy fails:** "Folder copy paused"; retried when the cave next changes or reopens.
- **Bring in changes clash:** the person chooses per item.
- **Server data loss:** restore from the nightly backup; devices that had a cave open upload what they hold
  (their pending entries, and an open snapshot when their copy is ahead).

## Testing

- Server: entries stored and returned as plain text; seats enforced (watcher refused, removed refused and
  stream closed, link expiry); migration of a locked cave to open; nightly backup produces a restorable file.
- Client: two devices on the real server in one process: edits both ways, offline then reconnect, stop
  guards; migration of a locked test cave.
- Folder copy: writes after quiet, paused on failure; Bring in changes shows differences and applies only
  accepted ones.
- Retirement: no Share/Join/Live/Classroom controls remain; old live links show the moved page; all
  remaining test suites pass.
- By Nathan: create a cave on one computer, open it on another, edit both ways; link a Drive folder and see
  the files appear; edit a file in the folder and bring it in.

## Build order

1. Open storage on the server and in the client; locked-cave migration.
2. The Cave Shelf lists server caves; new caves, imports and folder opens create server caves.
3. Retire Share, Live, Classroom, Join; new wording on the welcome page, privacy page and invite email.
4. Rooms, seats, hosts, links, waiting room, Everyone with access (rooms step 2, without keys).
5. Linked folder copy and Bring in changes.

## Replaced or changed elsewhere

- `2026-10-25-server-rooms-phase1-design.md`: storage and keys superseded by this spec (rooms, seats and the
  phase list stand).
- `2026-10-23-accounts-design.md`: the "server never holds cave data it can read" decision and the welcome
  note wording are superseded.
- `CLAUDE.md` rule "Never log or store entrance coordinates where a cave is shared": now means never log them
  and never put them in URLs or exports meant for sharing; the server does store them. Update the rule text in
  step 3.
