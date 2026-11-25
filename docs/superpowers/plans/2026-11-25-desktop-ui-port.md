# Desktop and iPad UI Port Plan (audit + plan)

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers-extended-cc:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax. One task = one commit. Do not start a task whose decision (D1-D7 below) Nathan has not answered; the tasks name the decision they wait on.

**Goal:** Carry the phone redesign's calmer, tighter, less wordy interface into the web app, the Tauri desktop app and the iPad, **inside the existing ribbon + dockable panel interface**. Only what helps where screen space is plentiful comes over; the phone's tab model does not.

**Architecture:** One codebase (`apps/web`). Desktop, web and iPad use the dock layout (`.shell.docked` in `App.svelte`: `Ribbon.svelte` over the dockview workspace in `lib/workspace.svelte.ts`); the phone layout (`phone.on`: width <= 600 or height <= 500, `lib/phone.svelte.ts`, `PhoneShell.svelte`) is untouched by this plan. The ported pieces are: (1) helper text moved out of the panels into the Handbook (each panel already has a `?` button in its dock header), (2) the phone's settings Registry (`lib/plugin.ts`, `lib/settings-sections.ts`) reused inside the existing user window (`UserMenu.svelte`) so settings gather in one place and the View tab gets lighter, (3) the live shot strip (`PhoneLiveStrip.svelte`) as an optional overlay on the Map panel when the Survey panel is not showing, (4) the share sheet for exports on the iPad (`lib/deliver-file.ts`).

**Tech stack:** Svelte 5 + TypeScript (`apps/web`), node `--experimental-strip-types` checks (`npm run test:*`), `npx svelte-check`, Tauri shell (`apps/desktop`, no Rust change expected), headless screenshots at 1440x900, 1180x820 and 820x1180.

**User decisions already made:**
- "let's port over our interface changes to the webapp and desktop app"
- "the desktop and ipad app are getting too crowded as well" (a calmer, tighter, less verbose UI is wanted there too)
- "I would like the webapp and desktop app to maintain the panel style interface where screenspace isn't as much of a concern." So the ribbon + dock stays on web, desktop and the iPad. **Rejected:** replacing the ribbon/dock with the phone's top tab row.
- The standing rules in `CLAUDE.md` bind every task here and are not changed by this plan: the ribbon has ONE fixed height (`--ribbon-h`, `--ribbon-body`, `--ribbon-name` in `app.css`; new ribbon content fits two short rows); settings of a running command go in the contextual ribbon tab (`commands.ts` `optionsFor`, `option()`, Done and Escape; never per-panel buttons for them); every editable panel has its own Undo/Redo scope (`PanelUndo`); units are set in the View tab and every length goes through `formatLength` / `LengthField` / `parseLength`; no generic CAD commands; Pitfall is the default test cave and Plumbline Pit (and Stairstep) guard vertical caves; the canvas is 3D; refresh-is-enough in dev.

## Baseline (audit, 2026-11-25, build b462829, dev server, Pitfall open, Survey/Symbols/Notebook/Stations panels shown)

Screenshots (not committed): `/private/tmp/audit-shots/` — `{desktop-1440x900,ipad-1180x820,ipad-820x1180}-{Home,Draw,Survey,Annotation,Sheets,View}.png`, `-user-options.png`, `-user-account.png`, and `{desktop-1440x900,ipad-820x1180}-panel-{Symbols,Notebook}.png`; numbers in `measurements.json`.

| Measure | 1440x900 | 1180x820 | 820x1180 |
|---|---|---|---|
| Ribbon (tab row + bar) height | 134 px (bar `--ribbon-h` 98) | 134 | 134 |
| Home: groups / visible controls / folded groups | 8 / 20 / 0 | 8 / 20 / 0 | 8 / 19 / 1 |
| Draw | 14 / 74 / 3 | 14 / 71 / 3 | 14 / 68 / 4 |
| Survey (dev builds only) | 8 / 26 / 0 | 8 / 26 / 0 | 8 / 23 / 1 |
| Annotation | 2 / 4 / 0 | same | same |
| Sheets | 8 / 24 / 0 | 8 / 24 / 0 | 8 / 19 / 2 |
| View | 20 / 68 / 3 | 20 / 59 / 6 | 20 / 57 / 10 |

- The crowding is the **number of panels at once** (Map is ~150 px tall at 1440x900 with Sheet, Device Capture, the Stations stack, Profile and Scans open) and **panel helper text**, not padding: dock panel paddings are already 3-10 px (most common `6px 10px`, `6px 8px`, `8px`), gaps 2-10 px (most common 4, 6, 8). The phone tokens (`--p-gap 4`, `--p-pad 6`) would change little; no new desktop spacing tokens are proposed.
- Helper/explainer elements (`class="hint|help|note|idle|banner|sub"`) in desktop-visible files (Phone*/Settings* excluded): SurveyPanel 15, AdminSection 7, ScansPanel 6, PatternsPanel 5, RoomsPanel 4, DownloadsPanel 4, AccountGate 4, CaveShelf 4, DrivesSection 3, AccountSection 3, StationsPanel 2, NotebookPanel 2, UserMenu 2, SurfaceData 2, NewCave 2, and 1 each in SymbolsPanel, SketchPane, SheetPanel, SectionPanel, ProfilePanel, MotifEditor, LineEditor, FillEditor, X1Marks, SurveyOptions. Visible on screen at 1440x900: Survey panel 6 paragraphs (e.g. the Cavway sweep explainer, the X1 M-mark explainer, "Or hold DIST+M…", "Connect an instrument in the Survey tab, or try the simulator."), Scans panel's "Add your scan pages…" (5 lines), Symbols "Pick a symbol, then click in a view.", Sheet "Plotted at 1" = 80 ft" (a value, keep), user window Options "Theme and colours are on the View tab."
- Long tooltips used as documentation: Ribbon.svelte 17 `title`s (e.g. Boulder brush, Ink style, Pick entrance: one to three sentences each), SurveyOptions 5, ThemeGroup 3, SimulatorOptions 2. Tooltips are fine on desktop (hover), absent on iPad (touch).
- At 820x1180 (iPad portrait) the View tab folds 10 of its 20 groups; that tab is the main ribbon crowding.

## Decision table

Applies: **D/W** = desktop app and web app (dock), **iPad** = iPad (dock, any orientation), **none** = stays phone-only.

| # | Phone change (commit) | Classification | Applies | What changes on the dock | Risk |
|---|---|---|---|---|---|
| 1 | Top tab row Caves/Survey/Sketch/Notebook/Settings (7160b61, 82b059c) | DO NOT PORT | none | Nothing: Nathan keeps the ribbon + dock. | - |
| 2 | Gates: sign-in then a cave (439b706, `lib/gates.ts`) | DO NOT PORT (sign-in part is already shared: `AccountGate`) | none | Nothing. On the dock the Cave Shelf already opens at startup when no cave is open (`App.svelte`: `!phone.on && !cave.hasCave && shelf.startup`), with "Start empty" and the user's "Open this window at startup" switch; an empty dock is a valid state (Home > Caves/New/Open). | - |
| 3 | Registry (`lib/plugin.ts`) | PORT AS IS | D/W, iPad | Reused by Task 3 (settings sections in the user window). Already tested (`tests/registry.check.ts`). | low |
| 4 | No instrument simulator on the phone (cef5210) | DO NOT PORT (already gated) | none | Desktop keeps it behind `showInstruments` (`lib/features.ts`: dev builds, `VITE_UPDATE_CHANNEL=dev`, or `cavecad.instruments=on`). See D7. | - |
| 5 | Portrait only on iPhone (6ac6284) | DO NOT PORT | none | iPad keeps all orientations (already so in that commit). | - |
| 6 | Stations / Notebook one-screen `compact` (ca987c2, fcaa4d8) | DO NOT PORT | none | Behind `compact` props the dock never passes. | - |
| 7 | Sketch type dropdown (60bca3d) | DO NOT PORT | none | The Draw tab's groups already fold into dropdowns when narrow, and View > Ribbon settings > Compact layout folds them all. A single dropdown would replace the ribbon's buttons, which the panel-style decision keeps. | - |
| 8 | Symbol favorites + collapsible sections + NSS/UIS (60bca3d, `symbol-favorites*`, `symbol-palette.ts`) | ALREADY PORTED in this branch | D/W, iPad | `panels/SymbolsPanel.svelte` already uses them (stars on hover, Favorites first, folding headers). Only the one-line hint goes (Task 1). | low |
| 9 | Slim options strip; drawing defaults (`detail`, `finger`, `pressure`) moved to Settings > Sketch (e457cf1, 29da050) | CONFLICT with "settings of a running command go in the contextual ribbon tab" | D/W, iPad | Recommended: nothing changes on the dock: the contextual tab keeps every option (`ContextOptions` with no `omit`). See D4. | low |
| 10 | Settings sections (f60db1f, 6c18665, 29da050, b54a5a0, 1656ec7) | ADAPT | D/W, iPad | The user window (`UserMenu.svelte`) Options tab becomes a list of dock-flagged sections from the same Registry: Display (theme, accent, compact ribbon, reset layout; units mirrored, View tab keeps its Units group by rule) and Symbols (NSS/UIS). Connection, Survey, Notebook, Cave, Account, About stay phone-only (the dock has the Survey panel and ribbon, the Account tab, Home > Import and export). See D1, D2. | medium (shared CSS is scoped to `.phone .set`) |
| 11 | Survey on one screen; default shot type, one Take shot button (fec9a90, b54a5a0) | DO NOT PORT | none | The dock Survey panel has both shot buttons and the X1-scan sweep kind `select`; `survey.sweepDefault` already seeds the engine on every layout. | - |
| 12 | Live shot strip (58a0bd7, `PhoneLiveStrip.svelte`, `lib/phone-livestrip.ts`) | ADAPT | iPad mainly, D/W optional | An overlay at the bottom of the Map panel, shown only when instruments are on and the Survey panel is not visible; its detail button opens the Survey panel (`showPanel('survey')`) instead of phone Settings. See D3. | medium |
| 13 | Notebook Backsights visibility toggle (6c18665, `notebook-display*`) | DO NOT PORT (dock already better) | none | `NotebookPanel` (dock path) already shows the bs columns per trip when the trip has backsights or they were added (`backsights` derived, line ~206). See D6. | - |
| 14 | Settings > Cave: Surface Data, DXF and station CSV export (1656ec7, `phone-export.ts`, `deliver-file.ts`) | ADAPT (share sheet only) | iPad | Dock already has Home > Import and export (DXF, survey data, project zip) and the Stations panel CSV download. On the iPad, exports go through `deliverFile` (share sheet when `navigator.canShare` accepts files, else download). | low |
| 15 | Offline notice as an icon (b462829) | DO NOT PORT | none | Dock keeps the bottom pill (it covers nothing there). | - |
| 16 | No helper text, tight spacing tokens (`phone.css`, DESIGN note 2026-11-25) | ADAPT | D/W, iPad | Helper paragraphs move to the Handbook (panel `?` button) per D5; one-line status and error lines stay. No new tokens; panels keep their 4-10 px spacing. | low-medium (Handbook checks) |
| 17 | `optionsWithout`, `drawingDefaults` in `commands.ts` | PORT AS IS (no dock use) | none | Left as is; the dock calls `optionsFor` through `ContextOptions` with no `omit`. | - |
| 18 | Keyboard and accessibility | ADAPT (keep) | D/W, iPad | Nothing the dock relies on was removed by the phone work (the `compact` paths are separate). New dock UI must keep Escape/Done in contextual tabs, Ctrl/Cmd+Z per `PanelUndo` scope, tab order and `role="tablist"` arrow keys in the user window, and visible focus on the live strip's buttons. | low |

## Tasks (safe order, one commit each)

Before every commit: `cd apps/web && npx svelte-check && npm test` (runs every `test:*` in `package.json`), plus the task's own checks. Screenshots: run the dev server (`npm run dev -- --port 5174`), pass the sign-in gate in the page (set `account.member` and `account.phase = 'ready'` from `import('/src/lib/account.svelte.ts')`, hang `/api` requests), open Pitfall (`openSample('pitfall')` from `/src/lib/caves.ts`), capture 1440x900, 1180x820 and 820x1180, compare with `/private/tmp/audit-shots/`, and check the phone at 375x812 did not change. No Rust and no `apps/desktop` change is expected; if one happens, add the `apps/desktop` checks from `CLAUDE.md`.

### Task 1: Survey and Symbols panels without helper paragraphs (waits on D5)

**Files:** `apps/web/src/panels/SurveyPanel.svelte` (dock path only, i.e. `{#if !phone.on}` branches), `apps/web/src/components/SurveyOptions.svelte` (the `.note`), `apps/web/src/panels/SymbolsPanel.svelte` (the `.hint` line), `apps/web/src/components/UserMenu.svelte` ("Theme and colours are on the View tab."), the matching Handbook pages in `apps/web/src/handbook/pages/*.md` (Device Capture, Symbols; index in `handbook/index.json`).

- [ ] List every `.hint/.note/.idle/.banner/.simnote` in those files with its line and text in the commit message body.
- [ ] Move explainers into the Handbook page the panel's `?` opens (D5 option A); keep one-line status text (e.g. "Waiting for a shot…", "Simulating: …"), errors, and values.
- [ ] The phone path (`phone.on`) is not touched.

**Acceptance:** at 1440x900 with an X1 not connected, the Survey panel shows no paragraph longer than one line; the Symbols panel has no hint line; the Handbook pages hold the moved text. **Verify:** `npm run test:handbook && npx svelte-check`; screenshots `desktop-1440x900-Survey` and `ipad-820x1180-panel-Symbols` against the baseline.

### Task 2: Other dock panels without helper paragraphs (waits on D5)

**Files:** `apps/web/src/panels/ScansPanel.svelte` (6), `PatternsPanel.svelte` (5), `RoomsPanel.svelte` (4), `StationsPanel.svelte` (2), `NotebookPanel.svelte` (2, dock path), `ProfilePanel.svelte`, `SectionPanel.svelte`, `SketchPane.svelte`, `SheetPanel.svelte` (keep the "Plotted at" value), `components/CaveShelf.svelte` (4, window mode only), `components/SurfaceData.svelte` (2), `components/NewCave.svelte` (2), matching Handbook pages. Account screens (`AccountGate`, `AccountSection`, `AdminSection`, `DownloadsPanel`, `DrivesSection`) are out of scope: they explain sign-in and sharing, which is rare and consequential.

- [ ] Same rule as Task 1: explainers to the Handbook; empty states become one short line (e.g. Scans: "No scans yet." with the Add scans… button beside it).

**Acceptance:** no dock panel shows a multi-line explainer with a cave open. **Verify:** `npm run test:handbook && npm run test:stations && npm run test:notebook && npx svelte-check`; screenshots of Scans, Stations, Notebook at 1440x900 and 820x1180.

### Task 3: Settings sections in the user window (waits on D1)

**Files:** `apps/web/src/lib/settings-sections.ts` (add `readonly layouts: readonly ('phone' | 'dock')[]` to `SettingsSection`, and `settingsSectionsFor(layout)`), `apps/web/src/lib/settings-default.ts` (flag every existing section `['phone']`; register dock sections), new `apps/web/src/components/DockSettingsDisplay.svelte` and `DockSettingsSymbols.svelte` (dock-sized, own scoped styles: 28-30 px rows, 13 px text; do not reuse `.phone .set` rules), `apps/web/src/components/UserMenu.svelte` (the Options tab lists `settingsSectionsFor('dock')` as headed blocks; Account, Feedback, Admin stay), `apps/web/tests/registry.check.ts` (layout filter), `apps/web/src/components/PhoneSettings.svelte` (use `settingsSectionsFor('phone')`).

- [ ] Display: theme (dark/light/system) and accent, using the same functions as `ThemeGroup.svelte`; Compact ribbon; Reset the panel layout; Units as a mirror of the View tab's (same `setUnits`; the View tab keeps its Units group, units rule).
- [ ] Symbols: NSS/UIS through `setSymbolStandard` (`lib/phone-prefs.svelte.ts`, which writes the same `STANDARD_KEY` the Symbols panel reads); the Symbols panel's own NSS/UIS row stays (it is where the choice is used) unless Nathan says otherwise.
- [ ] Drawing defaults are NOT added (D4 A).

**Acceptance:** phone Settings still shows the same eight sections in the same order; the user window's Options shows Display and Symbols; changing the theme there updates the View tab's Theme group and vice versa. **Verify:** `npm run test:registry && npm run test:symbolfavorites && npx svelte-check`; screenshots `desktop-1440x900-user-options`, `ipad-820x1180-user-options`, and phone 375x812 Settings.

### Task 4: A lighter View tab (waits on D2; after Task 3)

**Files:** `apps/web/src/components/Ribbon.svelte` (the `View` groups), `apps/web/src/components/ThemeGroup.svelte` (kept, used by the user window or removed if unused), Handbook page for the View tab.

- [ ] Option B: remove the Theme group and the Ribbon settings group (Compact layout) from the View tab; both live in user window > Options > Display (Task 3). Units, Panels, Drawing scale, Map style, Rock halo, Map view, Layout and Saved layouts stay.
- [ ] Ribbon height unchanged (`--ribbon-h` untouched).

**Acceptance:** View tab 18 groups; at 820x1180 it folds fewer groups than the baseline 10; ribbon still 134 px on every tab. **Verify:** `npx svelte-check && npm run test:layoutslots && npm run test:docktidy`; View tab screenshots at the three sizes.

### Task 5: Live shot strip on the Map panel (waits on D3; instruments only)

**Files:** rename/generalise `apps/web/src/components/PhoneLiveStrip.svelte` into `LiveStrip.svelte` with a prop `onDetail: () => void` (phone passes `() => showSettings('connection')`, dock passes `() => showPanel('survey')`) and dock-sized styles behind a `dock` prop; `apps/web/src/components/PhoneSketch.svelte` (use the new name); `apps/web/src/panels/MapPanel.svelte` (mount it at the bottom of the canvas when `showInstruments && phonePrefs.liveStrip && !panelState.visible.survey`); `apps/web/src/lib/phone-livestrip.ts` (pure model unchanged, maybe a `visibleOnDock(...)` helper), `apps/web/tests/phonelivestrip.check.ts`.

- [ ] Take shot uses the same `live.pressShot` as the Survey panel; Escape and Ctrl/Cmd+Z in the Map keep working (the strip does not take focus on its own).
- [ ] The strip never covers the view cube, the scale bar or the map toolbar.

**Acceptance:** dev build, Survey panel closed: the strip shows on the Map; open the Survey panel: it hides. Production web build (instruments off): no strip. Checked on Pitfall and Plumbline Pit (a vertical leg shows clino, no invented bearing). **Verify:** `npm run test:phonelivestrip && npm run test:phonesurvey && npx svelte-check`; screenshots at 1180x820 with the Survey panel closed and open.

### Task 6: Share sheet for exports on the iPad

**Files:** `apps/web/src/components/ProjectFiles.svelte` (export menu: DXF, survey data, project zip), `apps/web/src/panels/StationsPanel.svelte` (CSV download), `apps/web/src/lib/deliver-file.ts` (already: share sheet when `navigator.canShare({ files })`, else download).

- [ ] First find where each export writes its browser download (not audited in detail: the export functions called from `ProjectFiles.svelte`, likely in `lib/files.ts`), then route those through `deliverFile`; the desktop app (`isDesktop()`) keeps its native save path unchanged.

**Acceptance:** desktop Chrome downloads as before; iPad Safari offers the share sheet. **Verify:** `npm run test:cavefile && npm run test:zip && npx svelte-check`; manual check on an iPad (note in the commit if not done).

### Task 7: Docs

**Files:** `docs/DESIGN.md` (a dated note: the dock keeps the ribbon and panels; what came over and what stayed phone-only, with the decision answers), Handbook pages already touched. `CLAUDE.md` unchanged unless Nathan approves a rule change (D2 C or D4 C).

**Verify:** `npm run test:handbook`; `python3 tools/make_code_index.py --check` (regenerate if a public TS item moved: it only indexes Rust, so expected no-op).

## Decisions needed from Nathan

**D1. Where do desktop settings gather?**
- A (recommended): the existing user window's Options tab lists sections from the phone's Registry, flagged for the dock (Display, Symbols). Does not change: the ribbon, the contextual tabs, the phone's Settings tab, the Account/Feedback/Admin tabs.
- B: a new dockable "Settings" panel in `PANELS`. Does not change: the user window; but it adds one more panel to a dock that is already crowded.
- C: no gathering; only remove helper text. Does not change: anything about where settings live.

**D2. Make the View tab lighter (it folds 10 of 20 groups on an iPad held upright)?**
- A: keep every View group. Does not change: anything in the ribbon.
- B (recommended): move Theme and Ribbon settings (Compact layout) to user window > Options > Display; Units, Drawing scale, Map style, Rock halo, Map view, Layout and Saved layouts stay. Does not change: the units rule ("The View tab's unit setting…"), the ribbon's fixed height.
- C: B and also move Units out of the View tab. Needs your approval to change the units standing rule's wording; nothing else changes.

**D3. The live shot strip on the dock?**
- A: no strip on the dock (the Survey panel is the place). Does not change: the Map panel.
- B (recommended): strip on the Map panel only while instruments are on and the Survey panel is not visible (mainly the iPad In-Cave layout with the Survey panel tabbed away), with an on/off in Settings. Does not change: the Survey panel, production builds while instruments are hidden.
- C: always on the Map while instruments are on. Does not change: the Survey panel; it duplicates it on a big screen.

**D4. Drawing defaults (Detail, Draw with finger, Pressure) on the dock?**
- A (recommended): they stay in the contextual ribbon tab with every other option of the running tool, as the standing rule says. Does not change: anything on the dock; the phone keeps them in Settings > Sketch.
- B: also show them in user window > Options (a second place to set the same value). Does not change: the contextual tab.
- C: take them out of the contextual tab on the dock too. Needs a change to the standing rule "Settings of a running command go in the contextual ribbon tab".

**D5. Where does the removed helper text go?**
- A (recommended): into the Handbook page each panel's `?` opens. Does not change: what the panels can do; tooltips stay.
- B: into tooltips (`title`) on the controls they explain. Does not change: the Handbook; but an iPad (touch) cannot see tooltips.
- C: delete it. Does not change: anything else; the knowledge is lost from the app.

**D6. Backsight columns in the dock Notebook?**
- A (recommended): keep the dock's automatic rule (columns show for a trip that has backsights or where they were added). Does not change: the phone's Settings > Notebook switch.
- B: the phone's switch also forces them off on the dock. Does not change: stored readings.

**D7. The instrument simulator in the Survey ribbon tab?**
- A (recommended): unchanged (dev builds, the admins' dev channel `VITE_UPDATE_CHANNEL=dev`, or `cavecad.instruments=on`). Does not change: anything.
- B: dev builds only (`import.meta.env.DEV`), so admins' test builds lose it. Does not change: the Survey tab otherwise.
- C: move it out of the ribbon into the Survey panel. Does not change: the simulator itself; frees ribbon width.

## What we will not touch

- The Rust engine (`crates/**`), `cave-wasm`, and the Tauri commands (`apps/desktop/src-tauri`).
- The phone layout (`PhoneShell.svelte`, `Phone*.svelte`, `phone.css`, `Settings*.svelte` content) except renaming the live strip in Task 5 and the Registry layout flag in Task 3.
- The ribbon's fixed height, the contextual-tab rule, per-panel Undo/Redo scopes, the units rule, test-cave rules and the 3D canvas: any change to them waits for Nathan's approval (D2 C, D4 C).
- Dockview itself and the saved-layout format (`lib/layouts.svelte.ts`).
- The account screens' explanatory text (sign-in, sharing, admin).
