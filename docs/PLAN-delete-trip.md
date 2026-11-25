# Plan: delete a trip (and everything it owns)

Status: built on `feat/delete-trip` (Rust, wasm, desktop and phone UI). Decided by Nathan 2026-11-25: removing a trip removes everything it owns.

Base: build on `origin/main` (branch `delete-trip`), never on the frozen `iphone-app` prototype. Line numbers below were read on the old base; re-find the spots on main before editing.

Phone UI is owned by the `phone-ux` branch (worktree `cavecad-rs-phoneux`). Split agreed with that session: this work lands the Rust, the wasm calls and the `engine.ts` wrappers first, plus the desktop (non-compact) UI. The phone's "Delete trip" row in the More sheet is added after `phone-ux` merges. Do not edit the compact markup of `NotebookPanel.svelte` on the delete-trip branch.

## Goal

A person can remove a trip from an open cave in the Notebook. One undo step (scope `notebook`) brings it all back. Nothing is left dangling: `cave_doc::check` reports no new `DanglingReference` after a delete.

## Today

- `cave-notebook` has `new_trip` and `remove_shot` ([edit.rs](../crates/cave-notebook/src/edit.rs)). There is no `remove_trip` in any crate, in `cave-wasm`, or in the web app.
- The Cave Shelf trip table is read-only. Shelf "Forget…" deletes a whole cave, not a trip.
- The Notebook trip chooser and + button are in `apps/web/src/panels/NotebookPanel.svelte` (lines ~186 and ~205).

## What a trip owns

Found by reading `crates/cave-doc/src/field.rs`, `check.rs` (the `REFS` table) and `cave-draw`.

| Owned thing | How it is tied to the trip | Delete rule |
|---|---|---|
| Trip entity | itself, with `TripScans`, `TripScanAlign`, `TripScanTrim`, start LRUD fields, `TripStartNote` | `Action::Remove` of `Kind::Trip` |
| Shots | `ShotTrip` = trip | remove |
| Stations | not tied to a trip; they are shared by label | remove only if **no surviving shot** (from another trip) uses it as `ShotFrom` or `ShotTo`. This takes its notes, marks, benchmarks and fixed coordinates with it. |
| Section features / symbols / text | `FeatureTrip` / `SymbolTrip` / `TextTrip` = trip | remove |
| Section features | `FeatureStation` in the removed-station set | remove (also when `FeatureTrip` is unset: `section_stroke` takes `trip: Option`) |
| Map lines | **no trip field.** `FeatureAnchored` holds `frames: Vec<(Id, Id)>` of station pairs | remove if **any** frame station is in the removed-station set |
| Profile sketches | **no trip field.** `FeatureRun` text key plus `FeatureAnchored` | same frame rule as map lines |

Kept on purpose:
- Stations used by shots of another trip.
- Scan image files on disk. Only the trip's links go. They are the person's files and the app cannot know whether another cave uses them.
- `Kind::Scan` entities (completion flag by path). They are keyed by path, not by trip. Leave them. Harmless.

Why "any frame station" for anchored strokes: a stroke with one frame on a removed station would re-anchor to a hole in the survey. Removing it is correct under "remove everything". A stroke that spans a deleted trip and a kept one is rare. The confirm dialog lists the count so the person sees it.

## Rules and guards

1. **Cannot delete the last trip.** A cave needs one. `EditError::Invalid("a cave needs at least one trip")`. The UI disables the button when only one trip exists.
2. **Carry the cave header forward.** `cave-shelf::summarize` ([lib.rs:80](../crates/cave-shelf/src/lib.rs)) reads the cave name (`TripCaveName`) and unit (`TripDistanceUnit`) from the **first** trip by order. If the deleted trip is the first one, copy `TripCaveName` and `TripDistanceUnit` onto the new first trip, in the same action list. Otherwise the cave turns into "Untitled cave" or flips units.
3. **One action list = one undo step.** Build every `Remove` and `Set` in one `Vec<Action>` and apply it through one `scoped_edit("notebook", ...)`. Undo and redo already work per scope.
4. **Order of removal** in the list: features and symbols and text, then shots, then stations, then the trip. This is not required for correctness (removal is by id), but it keeps the log readable.
5. **Sorted and deterministic.** Collect ids from `doc.ids_of(Kind)` (already ordered), so the action list is stable across runs.

## Steps

### 1. `cave-notebook`: `remove_trip`
File: `crates/cave-notebook/src/edit.rs`. Public surface: re-export from `lib.rs` next to `new_trip`.

```rust
/// What deleting a trip would remove, for the confirm dialog.
pub struct TripRemoval { pub shots: usize, pub stations: usize, pub drawn: usize }

/// Counts what `remove_trip` would take (no edit).
pub fn trip_removal(doc: &Document, trip: Id) -> Result<TripRemoval, EditError>;

/// Removes a trip with its shots, the stations only it uses, and everything drawn on them.
pub fn remove_trip(doc: &Document, trip: Id) -> Result<Vec<Action>, EditError>;
```

Share one private function `plan_removal(doc, trip) -> Result<Removal, EditError>` (sets of shot, station, feature, symbol, text ids, plus the carry-forward header) so the count and the delete cannot disagree.

Algorithm:
1. Check `trip` exists and is `Kind::Trip`, else `EditError::NoSuchTrip` (add the variant if absent; otherwise reuse the existing one). Check there are at least 2 trips.
2. `shots` = all `Kind::Shot` with `ShotTrip == trip`.
3. `kept_shots` = the other shots. `used` = every `ShotFrom` / `ShotTo` of `kept_shots`.
4. Candidate stations = every `ShotFrom` / `ShotTo` of `shots`. `stations` = candidates not in `used`.
5. Features: `Kind::Feature` where `FeatureTrip == trip`, or `FeatureStation` in `stations`, or an `Anchored` value has any frame station in `stations`.
6. Symbols and text with `SymbolTrip` / `TextTrip == trip`. (Check whether symbols or text can also be anchored to stations; if they can, apply the same station rule.)
7. Header carry: if `trip` is first by `TripOrder` (ties by id, same rule as `cave_notebook::trips`), find the next trip and add `Set TripCaveName` and `Set TripDistanceUnit` on it, only when it does not already hold a non-empty value.

### 2. `cave-wasm`
File: `crates/cave-wasm/src/lib.rs`, next to `notebook_new_trip` (~line 1577).

- `notebook_remove_trip(trip: &str) -> String`: `parse_id`, then `scoped_edit("notebook", |doc| cave_notebook::remove_trip(doc, id))`. Return `ok` or `error: ...`, like its neighbours.
- `notebook_trip_removal(trip: &str) -> String`: JSON `{shots, stations, drawn}` or `error: ...`. Read-only.

### 3. Web
- `apps/web/src/lib/engine.ts`: import both from `../wasm/cave_wasm.js`; add `notebookRemoveTrip` and `notebookTripRemoval` wrappers.
- Phone (later, on top of `phone-ux`): a "Delete trip" row inside the existing More sheet (`components/NotebookSheet.svelte`), not in the 2-row header. Short confirm: "Delete <name>? N shots, N stations, N drawn items. Undo brings it back." with two buttons. Disabled with one trip. Phone rules: no helper text, tokens `--p-gap` 4, `--p-pad` 6, `--p-row` 40, `--p-hit` 44, American spelling.
- `NotebookPanel.svelte`, desktop (non-compact) markup only: add a delete (trash) button beside the + at both sites. Disabled when `trips.length < 2`. On click, read the counts and show a confirm: "Delete trip *name*? This removes N shots, N stations and N drawn items. Undo brings it back." On confirm, call remove, then select the neighbouring trip (previous, else next). Reload the page chooser.
- Follow the repo rules: the dialog is our own component, not a generic command; no new bare number input; the panel already has `<PanelUndo scope="notebook">`.
- Cave Shelf: no change in v1 (table stays read-only).

### 4. Docs and index
- `python3 tools/make_code_index.py` and commit the result.
- Add a short entry to `docs/DESIGN.md`: trips can be deleted, what goes with them, what stays, the last-trip guard.

## Tests

In `crates/cave-notebook/tests/notebook.rs` (and unit tests in `edit.rs`):

1. **Pitfall Cave** (default fixture): delete a trip. Surviving trips' station positions and adjusted geometry are unchanged. `cave_doc::check` reports no `DanglingReference`.
2. **Plumbline Pit** (vertical guard) and **Stairstep**: the same, so no bearing is invented for pitches.
3. **Shared station**: build a cave where two trips meet at a station. Delete one trip. The shared station and its note survive.
4. **Anchored strokes**: a map line and a profile sketch anchored on the deleted trip's stations are removed. One anchored only on kept stations stays.
5. **Section feature** with `FeatureTrip` unset but `FeatureStation` on a removed station is removed.
6. **Header carry**: delete the first trip. The new first trip holds the cave name and unit, and `cave_shelf::summarize` still shows the same name.
7. **Last trip**: refused, and the document is unchanged.
8. **Count matches delete**: `trip_removal` equals what `remove_trip` removes.
9. **Undo**: apply the actions, then the scope undo in `cave-wasm` (or invert in a `cave-doc` test). Everything returns, including the trip's order.
10. **Convergence** (per CLAUDE.md): peer A deletes a trip while peer B adds a shot to it. Merge both orders. Both end identical, and the late shot appears in `check` as a `DanglingReference` (the known merge outcome). Document that outcome in the test.

## Checks before commit (CI is off, run by hand)

```bash
export PATH=/opt/homebrew/opt/rustup/bin:$PATH
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --target wasm32-unknown-unknown -p cave-id -p cave-store -p cave-log -p cave-geom -p cave-doc -p cave-survey -p cave-notebook -p cave-scans
python3 tools/make_code_index.py --check
cd apps/web && npx svelte-check
```

Then run the web app (`cd apps/web && npm run dev`) and delete a trip on Pitfall Cave in the Notebook, then undo it.

## Risks and open items

- **Merge hole:** a shot added to a trip another person deletes leaves a dangling `ShotTrip`. This matches how stations behave today (merge never fails; the Check tools show it). Not fixed here.
- **Profile sketch ownership** rests on the frame-station rule above, since sketches carry no trip. If a run spans two trips, a sketch is removed when it touches any removed station. Confirm with Nathan after trying it on Pitfall Cave.
- **Symbols and text anchoring:** unchecked whether they anchor to stations. Step 1.6 covers it. Read `cave-draw` and `cave-doc` for `SymbolAt` / `TextAt` first.
- **Desktop app and shelf:** no work needed. Both read the same engine.
- Size estimate: about 150 lines of Rust plus tests, about 40 lines of Svelte.
