# Handoff: in-cave survey (branch `survey`)

Built 2026-10-14 on the Banning PC with no instrument and no Rust build. Everything here is web-side
TypeScript; no crate was touched. Design and what exists: `docs/DESIGN.md`, section "Survey: shots from an
instrument straight into the notebook".

## Update 2026-10-14 (ported to Rust, branch `survey-rust`)

Everything below this note was written against a mock engine on a PC with no Rust build. It has since been ported to
Rust (see DESIGN.md, "The survey work moves to Rust") and run in the browser against the REAL engine: `npm run harness`
in `apps/web` opens a fresh cave; Simulator then Shoot station fills the real Notebook and the Plan, Profile and Cross
section from the document, Undo last shot takes a whole shot back in one step, and the console is clean.
**Where the text below says "mock", "stand-in notebook" or names a `.ts` file under `lib/instrument`, `survey.ts`,
`survey-run.ts`, `survey-writer.ts` or `sketch-geometry.ts`, read the Rust crate instead** (`cave-instrument`,
`cave-session`, `cave-sketch`). The open decision "single-edit shot in `cave-notebook`" is done.
Still NOT done: Web Bluetooth against a device, the Map beside these panels in the real app (the harness cannot show
it), `surveyLayout()` and the lock/layout edits to existing app files in the real app, and merging with `main`
(which has moved 39 commits since this branch was made; eight files conflict: `package.json`, `Ribbon.svelte`,
`Viewport3D.svelte`, `cave-view.ts`, `workspace.svelte.ts`, `ProfilePanel.svelte`, `SectionPanel.svelte`, `DESIGN.md`).

## What was checked, and how

- `npm run test:caveble`, `test:survey`, `test:surveywriter` (all in `apps/web`): the wire format decoded from a
  frame written out by hand from the reference client's source; acks, repeated frames, float noise; the
  session rules; the notebook writer against a stand-in notebook (including the rollback and undo paths).
- `npx svelte-check`: 0 errors.
- The panel itself was run in a browser against a MOCK of the wasm engine (the real one cannot be built on
  that PC): connect to the simulator, backsights, splay mode, undo, hold/accept/discard, passage fields. That
  harness is now `npm run harness` (see DESIGN.md), so anyone can tap through it.
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

The X1 decoder is written from the vendor PDF, manual and Windows app (`docs/CAVWAY-X1.md`), NOT yet checked against a
unit. Test in a Chromium browser on a laptop (Web Bluetooth works there), then on the iPad in Bluefy. Work through the
UNVERIFIED list in that note; the capture log (Survey panel, bottom) is how: it shows every frame as hex.

1. Survey panel > Connect X1. If the chooser does not show it, tick "Show every Bluetooth device" under the capture
   section and try again (the X1's advertised name and service list are not in any source). Check distance, compass
   and clino against the X1's own display, that Take shot fires it, and that three close shots become a leg.
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

## Update 2026-10-14 (later): the X1 decoder

CaveBLE turned out to be wrong for the X1; see `docs/CAVWAY-X1.md`. The decoder, the grouping of shots into legs, the
simulator and the X1 transport are built and tested (`test:cavway`, `test:legs`, `test:x1session`). Two numbers are
guesses to settle on the unit: how close shots must be to count as one leg (`isClose` in `legs.ts`), and the 8 second
pause after which one or two waiting shots become splays (`IDLE_FLUSH_MS` in `survey.svelte.ts`).

## Update 2026-10-14 (evening): the sketchpad

(Replaced: see the last update.) `panels/SketchPad.svelte` was a standalone prototype fed from plain shot and dimension lists, shown in the harness as a Sketch tab beside the Notebook tab. It is NOT wired into the real workspace: the real Map, Profile and Cross Section panels already draw and anchor strokes in the engine. Decide whether the pad replaces them for in-cave use (then it needs a data source from `cave-scene` and strokes saved through `cave-draw`, Rust on the laptop) or stays a look-and-feel study. Its strokes live in `localStorage` only. Not exercised on a real iPad: pen pressure and palm rejection are written from the pointer events and checked with synthetic events only.

## Update 2026-10-14 (night): scales, grid and panels

The sketchpad panes have paper scales (1" = 20' by default, dropdown per pane) and a fine graph-paper grid, and the harness is a dockview workspace (Survey, Notebook, Sketch tools, Plan, Profile, Cross section) like the app. Worth a look on the laptop and the iPad: whether the CSS-inch scale feels right (it will not measure an exact inch on a screen), whether the default 1" = 20' is useful for the cross section (a 3 m passage is about half an inch across: 1" = 5' reads better and is one dropdown away), and pen pressure and palm rejection with a real Apple Pencil. `lib/dock-parts.ts` is new and `workspace.svelte.ts` now imports from it: run the app and confirm the panels still dock, minimize, maximize and float as before (the move was mechanical, untested in the real app because the engine cannot be built on the PC).

## Update 2026-10-14 (late): the sketchpad is the app's own way of drawing

The harness now has the real ribbon (Draw and View tabs), the Draw tab's groups, the options and Edit tabs, `PanelUndo`, `LineEditor` and the rest, pointed at the survey (see DESIGN.md, "Sketchpad: reuse what the app has"). `Ribbon.svelte` gained an optional `tabs` prop; `SheetGroup` now uses `orderedScales` from `sheet.svelte.ts`; `workspace.svelte.ts` imports `SveltePanel` and `GroupActions` from the new `lib/dock-parts.ts`. These are the only edits to existing app files, all small, none exercised by the real app on this PC (no engine here), so on the laptop run the app and check: the ribbon is unchanged, the Sheets tab's Scale list is unchanged, and panels still dock, minimize, maximize and float.

To bring the pad into the real app: register `SketchPlanPanel`, `SketchProfilePanel` and `SketchSectionPanel` in `workspace.svelte.ts` `PANELS`, and feed `sketch.setSource` from `engine.notebookPage` (every shot's readings and every station's L R U D are on the page). Whether the app wants these beside Map, Profile and Cross Section or in their place is Nathan's call; `cave-draw` would then own the strokes instead of `localStorage`.

## Update 2026-10-14 (night): map lock and saved layouts

Edits to existing app files, none exercised in the real app on this PC (no engine): `lib/viewport/cave-view.ts` (`setViewLocked`, `viewLocked`; `snapTo` and `orbitBy` return early while locked), `components/Viewport3D.svelte` (the padlock, the option effect), `components/Ribbon.svelte` (Map view group, Saved layouts group, Scans button), `lib/workspace.svelte.ts` (registers the layout host; the In cave layout turns the lock on). Check on the laptop with a real Map: with the lock on, drag with the mouse, one finger and the pen: it pans and never turns; the cube and Home do nothing; two fingers still pinch and pan; a drawing tool still draws. In the harness the lock was checked on a real `CaveView` (rotation off, left drag and one finger set to pan, snap, home and cube drag ignored, turning works again unlocked) but a synthetic pointer drag does not move three's controls either way, so real pointer behaviour is unchecked. Saved layouts were tested end to end in the harness (save, recall, lock restored, survives a reload, works from the ribbon's folded popup).

## Update 2026-10-14 (night, 2): the Survey tab and the instruments

New in the app's own files: `components/Ribbon.svelte` has a Survey tab (and Home's Survey group is back to its stubs); `SurveyPanel.svelte` lost its connection buttons and toggles. None run against the real engine on this PC. Check on the laptop: the Survey tab shows, Connect opens the browser's chooser listing only the instruments by name, the Simulator and "Shoot station" fill the Notebook, Map, Profile and Cross Section, A1 has its own passage dimensions, and the panel's dock has only the shot, undo and next-station buttons.

Instruments are written from their open repositories and TopoDroid (`docs/INSTRUMENTS.md`); none has been connected. With a real unit, work through the UNVERIFIED lists in `docs/INSTRUMENTS.md` and `docs/CAVWAY-X1.md`, using the capture section (Survey panel) for anything unexpected. BRIC memory download (the `LAST_TIME` write) is not built, so shots taken while the page was not connected are not fetched.

## Update 2026-10-14 (night, 3): simulating each device

Survey tab > Simulator: pick Cavway X1, SAP6, SAP5 or BRIC5 and a quirk, press Simulator, then Shoot station. Worth trying each on the laptop with the real Map: the log (capture section of the Survey panel) lists every frame and any note the decoder made (ignored, missed, calibration). The one heuristic to know about is `looksLikeBack` (unmarked backsights for the instruments that cannot mark one).
