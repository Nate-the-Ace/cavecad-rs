# Document model (cave-doc)

Approved 2026-10-11 (approach A, delegated to Claude).

**What it is.** The in-memory cave: trips, stations, shots (survey data). Drawing entities and
layers come in the next crate.

**Three layers.** Authored survey data (stored) / authored drawing data (stored) / derived data
(never stored: positions, adjustment, walls, shaped lines, fills).

**Edits.** Every change is an `Action` (`Create`, `Remove`, `Set field`) stamped with a logical
`Stamp` (counter, actor). Fields are last-writer-wins registers, so applying the same set of ops in
any order, any number of times, gives the same document. Undo is a new edit that restores the
previous values. Undo is per actor (a `History` holds only that actor's edits).

**Why not a CRDT library now.** The op vocabulary is what makes merging possible; a library can be
adopted later behind it without changing callers.

**Safety net.** `check::validate` reports dangling station references and duplicate station labels
(what two offline users can create). Merges never fail on these; the Check tools surface them.

**Not here yet.** Drawing entities, layers, geometry, DXF, file splitting per trip.
