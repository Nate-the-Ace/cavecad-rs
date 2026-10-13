# Handoff: Classroom (branch `classroom`)

Written on a PC where Rust cannot be built, so the **engine was not rebuilt or run here**. This branch
changes no Rust: it is the web app and the relay. What was checked here: `npx svelte-check` (clean),
`npm run test:relay` (real sockets, includes a full class), `npm run test:classroom` (read/write list of
the engine, transcript, links, view mirroring) and the rest of `npm test`, except `test:scandraw`, which
fails on this machine before my changes too (a module import without an extension under Node 24).
**Nothing was looked at in a browser.** Your job: run it, try the class with two or three browser
windows, and fix what you see.

## Try this (three windows: one teacher, two students)

1. `cd apps/web && npm install && npm run dev`. Open a cave (Pitfall Cave from the Samples) in window 1.
2. Window 1: Home > Live > **Classroom**. Name it, Start. The drawer on the right shows a link: copy it.
3. Windows 2 and 3 (use a private window, or another browser, so they are separate people): open the link,
   type a name, Join as an observer.
4. Check, in this order:
   - Students see the teacher's cave. Click, type, drag, wheel in a student window: nothing may change,
     and the student's own undo (Ctrl+Z) does nothing. Open the console (`__classroom`) if you want to
     poke at it.
   - Teacher moves: switches ribbon tab, picks a drawing tool, opens or moves a panel, orbits and zooms
     the Map, pans and zooms the Profile and a Cross Section, picks another trip or station. Students follow
     within about a quarter of a second. Look especially at (a) a student window of a different size or
     shape and (b) rearranging panels (a rebuild of the workspace must not leave the views blank).
   - Pointers: moving the mouse in a student window shows that student's name and pointer in the
     teacher's view (Map, Profile, Section), not in the other student's window. The teacher's pointer
     shows in the students'.
   - Raise a hand in window 2, then window 3: the teacher's drawer lists them in order with the time
     waiting. "Call on" lowers the hand. Type a question as a student; it shows for everyone.
   - Teacher: Download log gives a text file of everything; `classroom-logs/` in the repo root gets a
     `.jsonl` as it goes. End class: chat stops, the record stays.
   - A student reloads: they get the link's page again, join again, are put back on the teacher's view
     and are given the earlier chat.
5. Also try: a second teacher starting a class with the same name (refused, and their own cave must not
   be replaced); a student opening the link before the teacher started (refused).

## And the same class direct, with no server

Start a Classroom, choose **Direct**: the Share dialog opens, make an invite, paste it into a second
window's Cave Shelf > Join, send the reply code back, Connect (students are let in at once). Everything in
the list above should behave the same: observer-only, the teacher's view followed, pointers, hands, chat,
log download (no file on disk in this mode). Also check that a student's edits are refused (try dragging in
the student window, and `__p2p` is in the console), and that closing the teacher's tab frees the student.

## Where things are

- The rules, shared: `apps/web/src/lib/classroom-room.mjs` (with `.d.mts`). Relay: `apps/web/scripts/classroom-relay.mjs` (new, thin) and the hooks in `sync-relay.mjs`; p2p: `startClassHosting`, `sendClass`, `classIo` in `p2p.svelte.ts`; the dev server
  passes the log folder in `vite-plugin-cave-sync.mjs`.
- Client: `apps/web/src/lib/classroom.svelte.ts` (state, start/join, hands, chat, the teacher's beat,
  the student's input blocker), `classroom-mirror.ts` (the registry), `classroom-log.ts` (transcript,
  links), `readonly.ts` (the engine guard; `engine.ts` now exports the guarded engine).
- UI: `components/ClassroomDock.svelte` (drawer), `ClassroomDialogs.svelte` (Start, and the student's
  Join page), a Classroom button in the ribbon's Live group, mounted in `App.svelte`.
- Mirror parts registered in: `Viewport3D.svelte` (Map camera), `ProfilePanel.svelte`,
  `SectionPanel.svelte` (their `fit()` is skipped for a student), `Ribbon.svelte` (tab),
  `workspace.svelte.ts` (layout), and `classroom.svelte.ts` (trip, run, station, tool, ...).
- Decisions and what is not built: the "Classroom" entry in `docs/DESIGN.md`.

## Known weak spots, in the order I would look

- `fromJSON` of the teacher's layout on a student: panels remount, their registered views get the state
  again through the registry, but I have not seen it.
- The Map camera in a window of another shape (it shows at least what the teacher shows).
- The input blocker lets browser shortcuts through (reload, copy, zoom) and blocks the rest, including
  Tab and Escape; anything outside `[data-classroom-ui]` is dead to a student.
- The Docker server branch (`claude/exciting-gates-whufst`) edits `sync-relay.mjs` too; merging it with
  this branch will conflict there. It also needs a way to set the class log folder.
