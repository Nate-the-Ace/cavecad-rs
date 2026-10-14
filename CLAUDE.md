# CaveCAD (Rust)

Cave-survey mapping app, rebuilt from scratch in Rust. Not QCAD, not CADCraft. The QCAD-era app is
frozen at tag `qcad-final-2026-10-11` in the old repo. Direction and decisions: `docs/DESIGN.md`.

## Before you search, read the index

`docs/CODE-INDEX.md` lists every crate, module and public item with `file:line`. Look there first.
After you add, move or rename a public item, run `python3 tools/make_code_index.py` and commit the
result. CI fails if the index is stale. A good one-line `///` doc on every public item is what makes
the index useful; `missing_docs` is a warning for that reason.

## Rules (each one is load-bearing)

- **Modular crates.** One job per crate, `cave-*` names, small public surface. Tools depend on core
  crates, never on each other. No god crates.
- **wasm32-safe core.** Core crates do no file, thread, clock, or OS-entropy access. Anything
  environmental comes in through a trait (`Store`, `IdSource`, a log sink). CI builds the core for
  `wasm32-unknown-unknown` on every commit.
- **No generic CAD commands.** Users act only through our own tool buttons. Do not build a command
  line, generic draw/modify tools, or dimensions.
- **Stable ids.** Everything a person authors has a `cave_id::Id`. Never reference by list position
  or shared counter. Station labels are display text, not identity.
- **Edits are operations** (serializable, with actor and logical clock, never wall-clock time).
  This is what makes undo, history and later co-editing possible.
- **Source vs derived.** Store only what a person authored. Anything computed (adjustment, shaped
  lines, fills, profile views) is regenerated deterministically, with seeded randomness.
- **Never log or store entrance coordinates** where a cave is shared. `cave-log` redacts as a
  safety net, but do not rely on it. The per-cave `<cave>.log` is excluded from sanitized packages.
- **Tests with every change.** Convergence (order-independence) tests for anything merged.

## Commands

    cargo test                                   # all crates
    cargo clippy --all-targets
    cargo fmt
    python3 tools/make_code_index.py             # regenerate the index (CI uses --check)

Toolchain: use rustup's, not Homebrew's `rustc` (no wasm32 target). Put it first on PATH:
`export PATH=/opt/homebrew/opt/rustup/bin:$PATH`. Local browser-build check:

    cargo build --target wasm32-unknown-unknown -p cave-id -p cave-store -p cave-log -p cave-geom -p cave-doc -p cave-survey -p cave-notebook -p cave-scans -p cave-pattern

## Web app (apps/web)

    cd apps/web && npm install && npm run dev      # http://localhost:5173

Refresh-is-enough rule: dev and preview send `Cache-Control: no-store`, no service worker is kept
in dev, and the Vite plugin (`apps/web/scripts/vite-plugin-cave-wasm.mjs`) rebuilds the WASM when
any `crates/**/*.rs` changes, holds requests while it builds, then reloads the page. The footer
shows the git hash and build time of what the page runs. Production: hashed assets are immutable,
`index.html` and `build-info.json` are never cached (`public/_headers`). Never add a service worker
that serves files cache-first.

Panels: `apps/web/src/lib/workspace.svelte.ts` (dockview). Add a panel by adding an entry to `PANELS`.
Panels are dockable, resizable, tabbable, floatable, maximizable, minimizable and closable; the
View tab brings closed ones back; layout is remembered in localStorage (a stale layout is dropped).
Dockview styles only ship in its all-in-one bundle, so the import path is deliberate.

## Desktop app (apps/desktop)

The same web app in a native Tauri window (decided 2026-10-11), for what a browser may not do: the real path of a picked folder, pages read straight from it, Reveal in Finder/Explorer. It is NOT in the engine's workspace (own `[workspace]`; the engine must build for the browser). In the web app, `lib/desktop.ts` is the only door (`isDesktop()`); every caller keeps a browser fallback. Commands live in `apps/desktop/src-tauri/src/lib.rs`; the path-safety and folder reading are in `files.rs` with tests.

    cd apps/desktop && npm install && npm run build:debug     # builds apps/web, then the app (target/debug/bundle/macos/CaveCAD.app)
    cd apps/desktop/src-tauri && cargo test && cargo clippy --all-targets -- -D warnings

The build removes `apps/web/dist/local` (real cave data, never committed) before bundling. Add the two commands above to the checks before a commit that touches `apps/desktop`.

## Reuse the old math, prove it with the old answers

The QCAD-era app already solved the survey math. Port from `~/Documents/github/cavecad-main/cave-survey/scripts/CaveSurvey/Core/` (read the comments: they record hard-won decisions), then prove parity: `tools/golden/gen_survey_golden.mjs` runs the OLD JavaScript on its synthetic test caves and writes answers to `crates/*/tests/golden/`; Rust tests must match them. The old `tests/js_unit.js` has hand-solved numbers worth porting as unit tests.

## CI is switched off

The `ci` workflow is disabled on GitHub (Nathan's call, 2026-10-11: webapp-forward, no more failure emails). The file stays in `.github/workflows/ci.yml`; re-enable with `gh workflow enable ci`. Until then the checks are run by hand before every commit: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, the wasm32 build line above, `python3 tools/make_code_index.py --check`, and `npx svelte-check` in `apps/web`.

## Test caves (standing rule from Nathan, 2026-10-11)

**Pitfall Cave is the default test cave** (opens first in the app; the main fixture for every check). **Plumbline Pit is the vertical-cave guard**: every decision about geometry, walls (LRUD), cameras or views must also be checked on Plumbline Pit (and Stairstep) so horizontal and vertical-style caves both keep working. A pitch has no bearing, so nothing may invent one (see the old app's "vertical bearing trap").

## The canvas is 3D

The whole canvas is a 3D view (three.js, `apps/web/src/lib/viewport/`): z is up, y is north. Plan is just the Top view. The view cube snaps to 26 directions; a flat face view switches to orthographic. Dev hook: `window.__view` in dev builds. Scene data (always 3D) comes from `cave-scene` through `cave-wasm`.

## Roadmap items every step must respect

Xrefs and multi-user co-editing are not built yet but are on the road map; `docs/DESIGN.md` ("Standing considerations") lists the gaps and a per-step checklist. Read it before adding a tool or crate.

Pen input is on the road map too (see "Pen and tablet input" in `docs/DESIGN.md`): strokes keep optional `widths`; pressure never moves a coordinate.

## Units (standing rule from Nathan, 2026-10-12)

The View tab's unit setting (metric by default, or imperial) decides what people read AND what they type. Every length a tool shows goes through `formatLength` and every length a tool asks for uses `apps/web/src/components/LengthField.svelte` (parser: `parseLength` in `lib/units.ts`, which also accepts an explicit unit such as `30 cm` or `5'6"`). Never add a bare number input for a distance. Fields hold metres internally; the stored cave and DXF export stay metric. Check the parser with `npm run test:units` in `apps/web`.

## Every panel that can be edited has its own Undo and Redo (standing rule from Nathan, 2026-10-12)

A panel that can be drawn on or otherwise edited (Map, Profile, Cross Section, and any future one: Shaped Lines, Callout, sheets, ...) keeps its own history and must never undo another panel's work. In the engine an edit is recorded with `record_edit(scope, ...)` where `scope` is the panel (`Scope` in `apps/web/src/lib/engine.ts`: add the new panel's name there and in `cave-wasm`); in the panel's toolbar put `<PanelUndo scope="..." label="..." />` (`apps/web/src/components/PanelUndo.svelte`) and handle Ctrl/Cmd+Z and Shift+Ctrl/Cmd+Z in its keydown with `engine.undo(scope)` / `engine.redo(scope)`. Never call undo or redo without a scope.

## Settings of a running command go in the contextual ribbon tab (standing rule from Nathan, 2026-10-12)

When a command is running the ribbon shows a tab for it with its common options. Define a command's options in `apps/web/src/lib/commands.ts` (`optionsFor`), read them with `option(id, default)` from `lib/cave.svelte.ts`, and never add per-panel buttons for them. Give the tab a Done path too (the Done button, and Escape).

## The ribbon has one fixed height (standing rule from Nathan, 2026-10-12)

The ribbon never changes height, whatever tab or contextual tab is showing, so the panels below it never move. `--ribbon-h` (the bar), `--ribbon-body` (a group's controls) and `--ribbon-name` (a group's name) in `apps/web/src/app.css` set it, and everything inside is sized from them: the bar clips, each group fills its height, a button is `calc(var(--ribbon-body) - 1px)` tall. New ribbon content (a group, a custom component, a contextual tab) must fit inside `--ribbon-body` (two short rows of controls at most) and use these variables; never rely on content to set the ribbon's height.

