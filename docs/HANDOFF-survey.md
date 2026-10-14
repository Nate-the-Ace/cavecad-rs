# Handoff: in-cave survey (branch `survey`)

Built 2026-10-14 on the Banning PC with no instrument and no Rust build. Everything here is web-side
TypeScript; no crate was touched. Design and what exists: `docs/DESIGN.md`, section "Survey: shots from an
instrument straight into the notebook".

## What was checked, and how

- `npm run test:caveble`, `test:survey`, `test:surveywriter` (all in `apps/web`): the wire format decoded from a
  frame written out by hand from the reference client's source; acks, repeated frames, float noise; the
  session rules; the notebook writer against a stand-in notebook (including the rollback and undo paths).
- `npx svelte-check`: 0 errors.
- The panel itself was run in a browser against a MOCK of the wasm engine (the real one cannot be built on
  that PC): connect to the simulator, backsights, splay mode, undo, hold/accept/discard, passage fields. That
  harness was throwaway and is not in the branch.
- NOT run: the panel against the real engine, the other panels reacting to new shots, `surveyLayout()` (it
  is dockview code like `workLayout()`, not exercised), Web Bluetooth (`webble.ts` is written from the API
  without a device), any Rust.
- Known, not part of this work: `npm test` stops at `test:scandraw` (`scan-draw.ts` imports `./scan-warp`
  without `.ts`, which Node's runner cannot resolve; Vite can). The new tests were run on their own.

## First things to do on the laptop

1. `cd apps/web && npm install && npm run dev`, open the app, Home > Survey > "In cave" (or View > Layout), open a
   cave, press Simulator, then Take shot. Watch the Map, Profile and Cross Section grow. Run with
   Backsights on and off, and in Splay mode. Try Undo last shot, Write at once off, and the L R U D fields.
2. Check one thing in the notebook after a few shots: a backsight sits in the Az bs / Inc bs cells of the
   right line, and the page shows no "differ by" warnings for the simulator's backsights.
3. Check a plumb shot (clino 88 to 90) on Plumbline Pit: the reading is stored as sent and the page draws it
   as a pitch (the engine ignores the compass past 85 degrees).

## When the Cavway X1 is in hand

The CaveBLE protocol is verified against its reference client, NOT against the X1. Whether the X1 speaks it is
the open question. Test in a Chromium browser on a laptop (Web Bluetooth works there), then on the iPad in
Bluefy.

1. Survey panel > Connect. If the chooser shows the X1, it advertises the CaveBLE service
   (`137c4435-8a64-4bcb-93f1-3792c6bdc965`) and shots should appear. Check distance, compass and clino against
   the instrument's own display, and that Take shot fires it.
2. If the X1 is not offered: use a Bluetooth scanner (nRF Connect) to list its services and characteristics,
   paste the service UUIDs into "Instrument log and capture", and Connect in capture mode. Fire shots and
   copy the log: each line is the characteristic and the frame in hex. That is the sniffed protocol; a
   decoder is then one more small file beside `caveble.ts`, feeding `LegLink`-style readings.
3. Likely to need a look on the real device: whether it sends magnetic or true bearings and whether
   backsight clino sign and direction match the simulator's (the notebook expects a backsight to read about
   180 degrees from the foresight and the clino sign flipped); whether it needs the leg ack at all; reconnect
   behaviour (`webble.ts` retries after 0.5, 1.5, 4 and 8 s); iPad: pairing in Bluefy, and that Bluefy keeps
   the connection with the screen dimmed.
4. Classic Bluetooth (serial) instruments, like a DistoX, cannot be reached by any browser. If the X1 is one,
   the answer is a native wrapper, not this code.

## Open decisions for Nathan

- **Offline loading** (see DESIGN.md): a network-first service worker, or "load before the cave and do not reload".
- **Single-edit shot** in `cave-notebook` (Rust, laptop): one undo step per shot instead of several.
- **Write at once** is on by default (as TopoDroid does); the alternative is holding every reading for Accept.
