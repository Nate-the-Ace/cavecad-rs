// One instrument event, start to finish: decide what it is, write it to the notebook, and say where the
// surveyor stands afterwards. Pure (the notebook is a `NotebookPort`), so a whole trip from a simulated X1
// can be run and checked without a browser (tests/x1session.check.ts). The reactive store only holds the
// results of this and puts them on screen.

import type { LegEvent } from './instrument/legs.ts';
import type { Reading } from './instrument/types.ts';
import { advance, applied, decide, decideEvent, type Plan, type SessionState } from './survey.ts';
import { rewrite, write, type NotebookPort } from './survey-writer.ts';

/** What the store keeps about each thing written: enough to show it, to take it back, and to update it. */
export interface Entry {
  /** A counter, for list keys (set by the caller). */
  n: number;
  kind: Plan['kind'];
  /** Where it was written, as a surveyor would say it (`A3 to A4`, `splay at A4`, `backsight A4 to A3`). */
  label: string;
  reading: Reading;
  /** The shot it created (leg, splay) or completed (backsight). */
  shot: string;
  warnings: string[];
  /** Where the surveyor stood before it, so undoing it puts them back. */
  before: SessionState;
  /** The instrument's group, for a leg that may still gain shots. */
  group?: number;
}

/** Something to write: a single reading (the surveyor says what it is) or an event from the X1's grouping. */
export type Incoming = { kind: 'single'; reading: Reading } | LegEvent;

export type Outcome =
  /** Written: the new state and the entry for the log. */
  | { ok: true; state: SessionState; entry: Entry; warnings: string[] }
  /** A leg that gained a shot was rewritten: the new state and the entry with its new average. */
  | { ok: true; state: SessionState; updated: Entry; warnings: string[] }
  | { ok: false; error: string; state: SessionState };

/**
 * Handles one event. `names` gives the station names in use (called after a write too, as it has grown).
 * `log` is what has been written so far, to find the leg a `leg-update` belongs to.
 */
export function run(port: NotebookPort, trip: string, state: SessionState, incoming: Incoming, names: () => Set<string>, log: readonly Entry[]): Outcome {
  if (incoming.kind === 'leg-update') {
    const entry = log.find((e) => e.group === incoming.group);
    if (!entry) return { ok: false, error: '', state }; // taken back since: nothing to update
    const result = rewrite(port, entry.shot, incoming.reading, entry.kind === 'back' ? 'bs' : 'fs');
    if (result.startsWith('error')) return { ok: false, error: result.replace(/^error: /, ''), state };
    const updated: Entry = { ...entry, reading: incoming.reading };
    const last = state.last && state.last.shot === entry.shot ? { ...state.last, reading: incoming.reading } : state.last;
    const awaiting = state.awaiting && state.awaiting.shot === entry.shot ? { ...state.awaiting, reading: incoming.reading } : state.awaiting;
    return { ok: true, state: { ...state, last, awaiting }, updated, warnings: entry.warnings };
  }
  const { base, decision } =
    incoming.kind === 'single'
      ? { base: state, decision: decide(state, incoming.reading) }
      : decideEvent(state, incoming, names());
  if (!decision.plan) return { ok: false, error: decision.reject ?? 'That reading cannot be used.', state };
  const done = write(port, trip, decision.plan, base);
  if (!done.ok) return { ok: false, error: done.error, state };
  const group = incoming.kind === 'leg' ? incoming.group : undefined;
  const entry: Entry = { n: 0, kind: decision.plan.kind, label: done.label, reading: decision.plan.reading, shot: done.shot, warnings: decision.warnings, before: state, group };
  return { ok: true, state: applied(base, decision.plan, done.shot, names()), entry, warnings: decision.warnings };
}

/** The surveyor gives up the backsight being waited for (or moves on without a shot): they stand at the next station. */
export const moveOn = (state: SessionState, names: ReadonlySet<string>): SessionState => advance({ ...state, to: state.to || state.from }, names);
