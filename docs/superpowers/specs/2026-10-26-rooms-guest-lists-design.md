# Rooms panel: rooms and guest lists per cave

Date: 2026-10-26. Status: layout approved by Nathan (in the Rooms redesign session); built on branch `rooms-guest-lists`, not deployed.

Nathan's words: "I want the rooms panel to be something completely different. I want a better way to add/remove
rooms for a cave and edit their guest list if I'm an owner, that's where we can add/remove users, per cave."
And: "I'm not really concerned about a user's individual devices. I just want the user to be manageable."

This is the first slice of build step 4 of `2026-10-26-server-caves-design.md` (rooms, seats), without links,
hosts, the waiting room or the Everyone with access tab.

## Decisions (Nathan, 2026-10-26)

- **Real sharing now.** Adding a person to a room's guest list gives them the cave: it shows on their Cave Shelf
  marked with whose it is, they open it and it stays in step. Taking them off stops new edits reaching them at once;
  the copy already on their computer stays there.
- **Two seats:** Editor (can change the cave) or Watcher (can look, cannot change). Picked per guest.
- **People, not computers.** The panel never shows computers, codes or keys. Send keys now is gone (keys only
  exist for caves locked before 2026-10-26 and are handed on by themselves).

## What people see

Rooms panel (Home > Live > Rooms, or the View tab), for the open cave:

- Top line: the cave's name, "you own this cave" / "you're an editor" / "you're watching", and the sync state.
- Left: the cave's rooms. **Private** first (owners only, always there, cannot be renamed, removed or given guests),
  then the rooms owners added, each with its number of guests. Owners: **Add a room** (asks for a name).
- Right, the selected room: its name (owners: rename), **Remove room** (owners; asks first), and the **Guests**
  list: name, email, a seat dropdown (Editor / Watcher) and a remove button. Under it, owners add a guest: type a
  name or email, pick a matching CaveCAD member from the drop-down, choose a seat, **Add**. Only members can be
  added (admins invite new people, as before). Owners are not listed as guests: they are in every room.
- Guests see the same panel read-only, with only the rooms they are in.
- A guest who is in two rooms of a cave gets the higher seat (Editor over Watcher).
- A watcher's cave is read-only on their computer ("You're watching this cave: you can look but not change it");
  opening or making another cave still works.
- A guest taken off every room: their open cave stops syncing with "You're no longer on this cave's guest list.
  Your copy stays on this computer but gets no new edits." It never turns into a new cave of theirs.

## Server rules (enforced on the server)

- Only owners add, rename and remove rooms, and change guest lists and seats.
- The Private room cannot be renamed, removed or given guests; room names are 1 to 60 characters, unique per cave.
- Guests can be added only to caves stored open (a cave still locked from before is opened once on the owner's
  computer, which converts it).
- Readers (snapshot, history, stream) are owners and guests. Writers (entries, snapshot, label) are owners and
  editors; a watcher's write is refused (403 `watcher`). Locked-cave key routes and conversion stay owner-only.
- Any change to a person's seats closes their open streams of that cave, so their app re-reads its seat at once.
  Losing every seat records them as gone from the cave (`GET /api/caves` lists gone caves) until added again.
- `GET /api/members/directory`: every current member's id, name and email, for picking guests; cave owners only.
- Only a cave's owners see email addresses; guests see names only (Nathan, 2026-10-26).

## Records

- `room_guests (room_id, member_id, seat editor|watcher, added_at)`.
- `cave_gone (cave_id, member_id, at)`.

## Testing

Server (`node --test`): owner-only changes; Private room rules; name rules; locked caves refuse guests; guests
list, read and stream a shared cave; watchers refused writes; removal closes streams and lists the cave as gone;
best seat wins. Client (`roomclient.check.ts`, real server): a guest syncs edits both ways; a watcher posts nothing
and gets edits; a removed guest stops with the removal message instead of making a new room.
By hand: Pitfall Cave and Plumbline Pit, owner adds a second member as editor, then watcher, then removes them.

## Inviting by email (Nathan, 2026-10-26)

"Make Add invite people who aren't members yet and seat them in the room when they join. Only admins can invite that
way; a non-admin owner gets 'ask an admin to invite them'."

- A room's Add takes a member picked from the list, or any email. A member's email seats them at once.
- Anyone else: an admin owner sends the normal invite (same email; a Copy link when the email fails) and the seat is
  kept for that email (`room_pending`). It becomes a `room_guests` row when they join through the invite
  (`members.ts resolve`). A non-admin owner is refused by the server (`admin-only`): "Ask an admin to invite them."
- Owners see kept seats on the guest list as "invited, not joined yet" with Cancel. A cancelled invite, an expired
  invite (no longer shown, never seats anyone), or removing the room drops them. Cancel drops the seat, not the
  invite (admins manage invites).
- Routes: `POST /api/caves/:id/rooms/:room/invited {email, seat}`, `DELETE .../invited?email=`.
