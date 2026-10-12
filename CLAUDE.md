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

Rust toolchain here is Homebrew `rustc`, which has no `wasm32` target; the wasm check runs in CI.
