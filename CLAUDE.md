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

    cargo build --target wasm32-unknown-unknown -p cave-id -p cave-store -p cave-log -p cave-geom -p cave-doc -p cave-survey

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

## Reuse the old math, prove it with the old answers

The QCAD-era app already solved the survey math. Port from `~/Documents/github/cavecad-main/cave-survey/scripts/CaveSurvey/Core/` (read the comments: they record hard-won decisions), then prove parity: `tools/golden/gen_survey_golden.mjs` runs the OLD JavaScript on its synthetic test caves and writes answers to `crates/*/tests/golden/`; Rust tests must match them. The old `tests/js_unit.js` has hand-solved numbers worth porting as unit tests.

## CI is switched off

The `ci` workflow is disabled on GitHub (Nathan's call, 2026-10-11: webapp-forward, no more failure emails). The file stays in `.github/workflows/ci.yml`; re-enable with `gh workflow enable ci`. Until then the checks are run by hand before every commit: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, the wasm32 build line above, `python3 tools/make_code_index.py --check`, and `npx svelte-check` in `apps/web`.

## Test caves (standing rule from Nathan, 2026-10-11)

**Pitfall Cave is the default test cave** (opens first in the app; the main fixture for every check). **Plumbline Pit is the vertical-cave guard**: every decision about geometry, walls (LRUD), cameras or views must also be checked on Plumbline Pit (and Stairstep) so horizontal and vertical-style caves both keep working. A pitch has no bearing, so nothing may invent one (see the old app's "vertical bearing trap").

## The canvas is 3D

The whole canvas is a 3D view (three.js, `apps/web/src/lib/viewport/`): z is up, y is north. Plan is just the Top view. The view cube snaps to 26 directions; a flat face view switches to orthographic. Dev hook: `window.__view` in dev builds. Scene data (always 3D) comes from `cave-scene` through `cave-wasm`.
