# Server rooms, phase 1: core rooms

Date: 2026-10-25. Status: design approved by Nathan, not built.

Live multi-user editing moves from peer-to-peer onto the CaveCAD server (`apps/server`, live at
https://cave.schonegg.stream). This is phase 1 of 7:

1. **Core rooms** (this spec): rooms that stay put, locked history, seats, links, waiting room, removal,
   presence carried over from today's live sessions.
2. Seats: driver seat (hotseat), claiming a trip or passage, status line, view frames, ghost shots,
   symbols and drags.
3. Talking: room chat, raise a hand, pinned notes on the map, @mentions, in-app notices, voice.
4. History: activity feed, restore an older version, named snapshots, replay by person or trip,
   try-out copy and merge.
5. Classes and notices: scheduled classes, class record, student roster, email digests, pop-ups.
6. Running the service: usage page, storage limits, admin action log, account download and delete,
   status page, off-site backups, shared symbol and pattern library.
7. Previews, PDFs and share pages through a per-cave owner switch that lets the server read that cave.

Later (no phase yet): offline edits sync later as a feature of its own, instrument shots streaming to a
room, a list of your devices, search across caves. Not wanted: none.

## Decisions (Nathan, 2026-10-25)

- **Every cave has a closed Private room from the moment it is made.** Its locked history lives on the
  server, which also backs it up and keeps your own devices in step. Owners open the cave up with
  roles, invites and more rooms.
- **A cave can have several owners.** The last owner cannot leave or stop being owner.
- **Extra rooms are the same cave with their own guest list and seats** (for example a class room where
  everyone watches and a team room where surveyors edit). Edits from any room land in the one cave.
- **Who a room link lets in:** members join straight in (or into the waiting room, if the room has one).
  Someone without a CaveCAD account signs in with Google and waits for an admin to approve their account
  before the link works. Only admins make new members, as before.
- **Who runs a room:** cave owners run every room; they can make someone a **host** of one room (a trip
  leader, a teacher). A host lets people in, removes them, changes seats and makes links in that room
  only. A host cannot delete the cave, add owners or touch other rooms.
- **Keeping a copy is a setting per room:** editors keep a copy on their device by default, watchers do
  not; owners can change either.
- **You see only the people in your own room** (pointers, ghosts, who's here). Owners also see a count
  and list of everyone in any of the cave's rooms.
- **Owners see everyone with access** to the cave, can change anyone's seats and can revoke all of a
  person's access in one step.
- **Per-device keys** (approach 1 below): the server never holds a key that opens a cave.
- Server reading is off for every cave in phase 1; the owner switch is phase 7. The first-sign-in note
  ("Your caves stay yours") stays true as written until then; phase 7 rewrites it.

## Approaches considered

1. **Per-device keys (chosen).** Each device has its own key pair; each cave has a lock key handed to
   each device that has access, wrapped so only that device can open it. Links carry a secret after `#`.
   Removal changes the lock. Delivers every decision above without the server reading caves. Most work;
   the lock change after a removal is the hardest part.
2. Key only in the link (today's peer-to-peer). Quick, but a forwarded link is a forwarded key; expiring
   links and removal cannot really be enforced; the key can never change.
3. Server holds the keys. Simplest, but the server could read every cave.

## What people see

**Rooms window** (Home > Live > Rooms; replaces Share and Join). Two tabs.

*Rooms tab:*
- Left: the cave's rooms. **Private** (owners only) first, then rooms the owners added (for example
  "Survey team", "Spring class"), each showing its host and how many people are in it now; **Add a
  room**. Under the list, owners only: "N people in this cave now".
- Right, for the selected room:
  - **People:** each person's seat (Owner, Editor, Watcher), a host label, a seat dropdown and
    **Remove** (for those the viewer may manage); people in the waiting room with **Let in** and
    **Turn away**.
  - **Links:** each link's seat, uses (one use or a number), expiry, **Copy**, **Stop**; **Make a link**.
  - **Settings:** Waiting room (someone lets each person in); Editors keep a copy on their device;
    Watchers keep a copy on their device.
- Hosts see only the room they host.

*Everyone with access tab* (owners only):
- Every person who has the cave's key, online or not: name, each room and seat ("Survey team: editor,
  host · Spring class: watcher"), whether they keep a copy, last seen.
- **Change:** set their seat in each room, add them to another room, make them a host or an owner.
- **Revoke all:** removes them from every room and changes the cave's lock. Confirmation: "Revoke all
  access for <name>? Removes them from N rooms and changes the cave's lock. They get no new edits. The
  copy already on their device stays there; CaveCAD can't take it back." Buttons Keep / Revoke.
- **Live links:** every link that still works, across all rooms, with **Stop**.

**Elsewhere:**
- **Cave Shelf:** a badge on a cave whose rooms have people in them ("3 here"); caves shared with you
  appear on your shelf, marked with whose they are.
- **In a room:** the avatars at the top right show only your room; a small room name beside them opens a
  switcher when you are in more than one room of the cave.
- **Admin:** a newcomer who opened a room link appears as a request: "<inviter> invited <email> to the
  <cave> <room> room". Approve lets them into CaveCAD and straight into that room; Turn away refuses.

## How it works

### Server (`apps/server`)

New records (SQLite, same database):

| Record | Holds |
|---|---|
| caves | id, created_at |
| cave_owners | cave, member |
| rooms | id, cave, name, kind (`private` or `shared`), waiting_room, editors_keep, watchers_keep |
| room_seats | room, member, seat (`editor` or `watcher`), host (yes/no) |
| waiting | room, member, since |
| links | id, room, seat, uses_left (null = unlimited), expires_at, wrapped cave key (locked with the link secret), stopped_at |
| devices | id, member, public key, created_at, last_seen |
| wrapped_keys | cave, key version, device, wrapped cave key |
| history | cave, number (1, 2, 3...), key version, locked entry, author member, room, received_at |
| snapshots | cave, up to number, key version, locked whole-cave copy |
| pending_signups | email, room, inviter, since |

Owners are seated in every room of their cave implicitly; the Private room has no other seats.

**Live connection:** one WebSocket per open cave at `wss://cave.schonegg.stream/rooms` (and `cave-api.`
for the desktop app), behind the same sign-in. Messages:
- *Edits:* locked entries. The server checks the sender's seat (watchers' edits are dropped), numbers
  each entry, stores it, and sends it to everyone in every room of the cave.
- *Presence:* pointers, ghost lines, selection, view. Locked too (they carry cave shape). Sent only to
  the sender's room, never stored, rate-limited per sender.
- *Catch-up:* "everything after number N" returns the missing entries in order, or the latest snapshot
  plus what follows it.
- *Room events:* who joined or left, waiting room, seat changes, removal, lock changes.

**The server can't read and still enforces:** it knows who is in which room with which seat, so it
drops watcher edits, refuses removed people, stops expired, used or stopped links, and refuses old app
versions (a version check on connect).

### Devices

- **Device key pair:** made on first sign-in with the browser's built-in encryption (WebCrypto X25519
  for wrapping, AES-GCM for entries); the private half is kept non-extractable in IndexedDB (desktop:
  the same, inside its webview). The public half is registered with the server.
- **Cave lock key:** made by the device that creates the cave (version 1). Wrapped for each device that
  has access and stored on the server; the server cannot unwrap any of them.
- **Adding a member or device:** an owner's or host's device (whichever is online, usually at "Let in" or
  when making a seat change) wraps the cave key for the new person's devices. A member's own new device
  gets the key from any of their other devices, or from any owner, host or editor device when it is next
  online.
- **Links:** making a link creates a random link secret, wraps the cave key with it, and stores that
  wrapped copy with the link. The secret goes in the link after `#`, which browsers never send to a
  server. Opening the link unwraps the key; the device then stores its own wrapped copy. A one-time,
  expired or stopped link has its wrapped copy deleted, so the secret opens nothing.
- **Removal and lock change:**
  1. The server cuts the person off at once (connection closed, seats and wrapped keys deleted).
  2. The next owner or host device online makes a new key version and wraps it for every device that
     still has access and every live link. Until then owners see "Lock change waiting for an owner's
     device".
  3. New entries use the new version. The removed person keeps what they already had; nothing new.
- **Keeping a copy:** if the room's setting says that seat does not keep a copy, the cave is wiped from
  the device when the person leaves or is removed (the classroom's existing wipe).
- **Watchers:** the engine is read-only for them (the existing read-only lock, its own source like the
  account lock).
- **Your folder copy:** the cave file in your folder keeps saving as today. The room history keeps
  everyone in step; offline edits merge in on reconnect with the existing order-independent merge.
- **Snapshots:** after a set number of entries, the device that crosses it uploads a locked snapshot so
  joiners do not replay the whole history.

### What it replaces

Peer-to-peer sessions (`p2p.svelte.ts`), the Apps Script signalling mailbox, and the Metered relay. The
peer-to-peer code stays in the repository, hidden from the screens, as a fallback until rooms have
proved themselves. The old `sync-relay.mjs` and classroom relay rules are reused where they fit
(`classroom-room.mjs` rules carry over to a class room).

## When things go wrong

- **Server unreachable:** keep working on the device copy; the room shows "Offline: your edits will join
  when you reconnect"; on reconnect, edits merge and catch-up runs.
- **Two people edit the same thing:** both edits kept, merged the same way on every device (claiming in
  phase 2 prevents it).
- **No owner or host device online after a removal:** the person is already cut off; the lock change
  waits and owners see the note above.
- **All your devices lost:** a new device has no cave keys. Another owner, host or editor of a shared room
  hands the key back when next online. A cave only you had: your folder copy still opens (the file is
  not locked, only the server's history); the Private room starts again from it. A cave that lived only
  on the lost device with no folder copy is gone.
- **Forwarded link:** members get in (or wait, if the room has a waiting room); newcomers wait for an
  admin; one-time links stop after one use; owners can stop any link.
- **Last owner leaves:** refused, "Make someone else an owner first."
- **Removed while in the room:** "You were removed from this room"; the cave is wiped if that seat does
  not keep a copy.
- **Old app version:** told to update before joining.
- **Server restart:** nothing lost; devices reconnect and continue from the last number.

## Testing

Automatic, on every change:
- **Server rules:** watcher edits dropped; removed people refused; one-time and expired links stop; last
  owner cannot leave; hosts cannot touch other rooms; catch-up returns exactly the missing entries in
  order; every stored entry, snapshot and presence relay is locked (a test fails on readable content).
- **Keys:** a device opens only what was wrapped for it; a link secret opens its room once; after a lock
  change a removed device cannot open new entries and every remaining device can.
- **Merging:** the same entries in any order, from any room, give every device the same cave.
- **Two devices end to end:** two copies of the app and the real server in one test: join by link, edit
  both ways, presence only within the room, removal, offline then reconnect.

By Nathan with one other caver: make a room and a link, they join, both edit, remove them, they get
nothing new; then a watcher room whose cursors never appear in the team room.

## Build order

1. Device keys and cave locks with only the Private room: your own devices stay in step through the
   server.
2. Extra rooms, seats, links and the waiting room.
3. Presence on the server: who's here, pointers, ghost lines, selection, follow view.
4. Removal and lock changes, the Everyone with access tab, handing over ownership.
5. Newcomers through room links waiting for admin approval.
6. Peer-to-peer retired from the screens.

## Not in phase 1

Storage limits (phase 6), the per-cave server-reading switch (phase 7), voice (phase 3), the driver seat
and claims (phase 2).

## As built (step 1)
- Live delivery is a streamed HTTP response (newline-delimited JSON over `fetch`), not a WebSocket.
- The stream always replays from a given entry number; the client fetches the snapshot itself.
- Snapshots are locked JSON `{upTo, ops}`; the `upTo` inside the lock is authoritative and must match the server's copy.
- Entries are locked JSON `{c: clientId, ops}`; a client skips its own entries by `c`.
- Device public keys are unique per member, not globally (another member holding the same key gets a device of their own).
- Streams are capped at 20 per member (the oldest is closed first).
- New-device approval is deferred (Nathan, 2026-10-25); devices trust the server-listed device list for now.
- Save as keeps the same room (it is the same cave written elsewhere); a future copy feature must clear the room id.
- No entry number is kept between opens: every open catches up from the snapshot (a cave copy can be older or newer than any number kept beside it). Reconnects within one open pass `snapshot?after=<last>` and skip a snapshot they are past.
- Unposted locked entries are kept per cave and client in IndexedDB; `stop()` takes the outbox at once (only while the engine still holds that cave) so the last flush window survives a session start. A cave switch replaces the engine's cave before `stop()`, so that window is covered by the next rule instead.
- A copy that changed while its room was closed posts itself whole (chunked) once on the next open. "Changed" = its SHA-256 differs from the hash kept when the client was live, quiet for 5 s, with nothing pending.
- Entries are split to stay under 1.5 MB locked; one op over that stops sync with `error` and a reason. A 400/403/404/409/410/413/422 on a post is `error` (the entry is kept); 401, 5xx and network failures are `offline` with backoff 300 ms doubling to 30 s.
- A cave whose room is not this member's at all gets a new room (said once). A member's own cave this device has no key for still waits (`waiting-key`), including after browser data is cleared with no other device left to hand the key on.
- Only a client that wrote entries since the last snapshot uploads the next one; a room with no snapshot (first upload failed) gets one from the next client to catch up.
- A stream silent for 60 s (pings come every 25 s) is dropped and reopened.
- Opening a cave on another device (2026-10-26): the Cave Shelf lists the member's caves on the server that its shelf does not keep (each shelf record keeps its `roomId`; older records read it once from the kept cave), under "On your other devices". Each cave has a **label** (`{name, summary}` as JSON) locked with the cave key (kind `label`, author `label`), set by any device that opens the cave when it differs from the kept one and again after the cave goes quiet; the server keeps it in `cave_labels` as base64 only. A device without the key lists the cave as "A cave from another device" until a key arrives. Opening one fetches the snapshot and the entries after it (the stream read until `live`), loads them, keeps it on the shelf and records its synced hash so the room client does not post it back whole. A cave with no key here waits ("Waiting for a key ...") and opens by itself once one arrives (the shelf re-reads the list every 15 s while open). Keys are handed on not only when a cave opens: a device also wraps every key it holds for the member's keyless devices when it signs in, every 10 minutes, and when its shelf lists server caves (`needsKeys` in `GET /api/caves` says which caves to check).
