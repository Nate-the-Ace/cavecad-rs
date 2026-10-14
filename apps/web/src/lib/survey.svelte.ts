// The live Survey session: ties an instrument link to the notebook. Readings come in from a link
// (Bluetooth, or the simulator), `survey.ts` decides what each one is, and this file writes it to the
// notebook through the engine's existing notebook calls, so every shot is an ordinary document edit
// (undoable, merged like any other, shown on the Map, Profile and Cross Section the moment it lands).
// Nothing here is stored in the cave: the session (where the surveyor stands, the mode, the log) is
// view state that is rebuilt from the notebook page when a trip is opened.

import { cave, changed } from './cave.svelte';
import { engine } from './engine';
import { SIDES, withSide, type NotebookPage, type Side } from './notebook';
import { SimulatedInstrument, traverse } from './instrument/simulator';
import { WebBleInstrument } from './instrument/webble';
import type { Instrument, LinkEvents, LinkStatus, Reading } from './instrument/types';
import { advance, applied, decide, nextStep, skipBack, standing, start, withMode, type Mode, type Plan, type SessionState } from './survey';
import { undo as undoWritten, write, type NotebookPort } from './survey-writer';

/** One thing written, with what is needed to take it back. */
export interface LogEntry {
  /** A counter, for list keys. */
  n: number;
  kind: Plan['kind'];
  /** Where it was written, as the surveyor would say it (`A3 to A4`, `splay at A4`, `backsight A4 to A3`). */
  label: string;
  reading: Reading;
  /** The shot it created (leg, splay) or completed (backsight). */
  shot: string;
  warnings: string[];
  /** Where the surveyor stood before it, so undoing it puts them back. */
  before: SessionState;
}

export type LinkKind = 'simulator' | 'cave' | 'capture';

export const survey = $state({
  status: 'idle' as LinkStatus,
  detail: '',
  device: '',
  canTakeShot: false,
  /** Where the surveyor stands and what they are doing. */
  state: start('', new Set()) as SessionState,
  /** Write each reading at once (default, as TopoDroid does) or hold it for an Accept. */
  auto: true,
  /** A reading waiting for Accept (only when `auto` is off). */
  pending: null as Reading | null,
  /** The latest reading, accepted or not, for the big display. */
  last: null as Reading | null,
  log: [] as LogEntry[],
  /** Raw frames, newest last, for working out an instrument whose protocol is not known. */
  frames: [] as string[],
  /** Why the last reading was refused (or the last edit failed). Empty when all is well. */
  message: '',
  /** Worth a look about the last reading, which was written anyway. */
  warnings: [] as string[],
  /** The trip this session was set up for: a different trip starts it again. */
  trip: '',
});

let instrument: Instrument | null = null;
let counter = 0;

/** Every station name in use anywhere in the cave and on this page: never suggest one that would close a loop by accident. */
function takenNames(page: NotebookPage | null): Set<string> {
  const taken = new Set(engine.scanViewStations('map', false).map((s) => s.label));
  for (const r of page?.rows ?? []) taken.add(r.label);
  return taken;
}

/**
 * Sets the session up for the open trip: standing at the last station on its page (or A1 on an empty
 * one), keeping the surveyor's mode and backsight choice. Called when the trip changes, not on every edit.
 */
export function syncToTrip() {
  if (!cave.trip || survey.trip === cave.trip) return;
  survey.trip = cave.trip;
  const page = engine.notebookPage(cave.trip);
  const last = [...(page?.rows ?? [])].reverse().find((r) => r.kind === 'station');
  survey.state = start(last?.label ?? 'A1', takenNames(page), survey.state);
  survey.log = [];
  survey.pending = null;
  survey.message = '';
  survey.warnings = [];
}

const fail = (m: string) => {
  survey.message = m.replace(/^error: /, '');
  return false;
};

/** The notebook calls the writer needs, answered by the real engine. */
const port: NotebookPort = {
  page: (trip) => engine.notebookPage(trip),
  addShot: (trip, from, to, splay) => engine.notebookAddShot(trip, from, to, splay),
  setReading: (shot, which, value) => engine.notebookSetReading(shot, which, value),
  removeShot: (shot) => engine.notebookRemoveShot(shot),
};

/** Writes a reading to the notebook as the session says it should go. Returns true when it was written. */
export function commit(reading: Reading): boolean {
  survey.pending = null;
  survey.last = reading;
  survey.warnings = [];
  const trip = cave.trip;
  if (!trip) return fail('Open a cave first.');
  const d = decide(survey.state, reading);
  if (!d.plan) return fail(d.reject ?? 'That reading cannot be used.');
  const before = survey.state;
  const done = write(port, trip, d.plan, before);
  if (!done.ok) return fail(done.error);
  survey.message = '';
  survey.warnings = d.warnings;
  survey.log = [...survey.log, { n: ++counter, kind: d.plan.kind, label: done.label, reading, shot: done.shot, warnings: d.warnings, before }].slice(-50);
  changed();
  survey.state = applied(before, d.plan, done.shot, takenNames(engine.notebookPage(trip)));
  return true;
}

/** A reading arrived from the instrument. */
function onReading(reading: Reading) {
  survey.last = reading;
  if (survey.auto) commit(reading);
  else {
    survey.pending = reading;
    survey.message = '';
  }
}

/** Writes the held reading (the Accept button). */
export const accept = () => survey.pending && commit(survey.pending);

/** Throws the held reading away (the Discard button). */
export function discard() {
  survey.pending = null;
}

/** Takes back the last thing written: the shot is removed (or its backsight cleared) and the surveyor stands where they did. */
export function undoLast(): boolean {
  const entry = survey.log[survey.log.length - 1];
  if (!entry) return false;
  const result = undoWritten(port, entry.kind, entry.shot);
  if (result.startsWith('error')) return fail(result);
  survey.log = survey.log.slice(0, -1);
  survey.state = entry.before;
  survey.message = '';
  survey.warnings = [];
  changed();
  return true;
}

function names(): Set<string> {
  return takenNames(cave.trip ? engine.notebookPage(cave.trip) : null);
}

export function setMode(mode: Mode) {
  survey.state = withMode(survey.state, mode, names());
}

export function setBacksights(on: boolean) {
  survey.state = { ...survey.state, backsights: on, awaiting: on ? survey.state.awaiting : null };
}

/** Gives up the backsight being waited for and moves on to the next station. */
export function skipBacksight() {
  survey.state = skipBack(survey.state, names());
}

/** Moves on to the next station without a shot (a station reached some other way). */
export function nextStation() {
  survey.state = advance({ ...survey.state, to: survey.state.to || survey.state.from }, names());
}

export const instruction = () => nextStep(survey.state);

// The passage dimensions at the station the surveyor stands at, written to the same cells the notebook shows.
function standingRow(page: NotebookPage | null) {
  const at = standing(survey.state);
  return [...(page?.rows ?? [])].reverse().find((r) => r.kind === 'station' && r.label === at && r.lrud !== null && r.lrud_editable) ?? null;
}

/** The dimensions (metres) at the standing station, or `null` before the page has a line for it. */
export function standingWalls(): { station: string; walls: Record<Side, number | null> } | null {
  const page = cave.trip ? engine.notebookPage(cave.trip) : null;
  const row = standingRow(page);
  if (!row?.lrud) return null;
  return { station: row.label, walls: Object.fromEntries(SIDES.map((s) => [s, row.lrud![s]])) as Record<Side, number | null> };
}

/** Writes one dimension at the standing station. */
export function setStandingWall(side: Side, value: number | null): boolean {
  const page = cave.trip ? engine.notebookPage(cave.trip) : null;
  const row = standingRow(page);
  const target = row?.shot ?? row?.station;
  if (!row?.lrud || !target) return fail('This station has no line on the page yet: take its shot first.');
  const result = engine.notebookSetWalls(target, withSide(row.lrud, side, value, false));
  if (result.startsWith('error')) return fail(result);
  survey.message = '';
  changed();
  return true;
}

// ---- the link ----

const events: LinkEvents = {
  status(status, detail = '') {
    survey.status = status;
    survey.detail = detail;
    survey.device = instrument && status === 'connected' ? instrument.name : survey.device;
    survey.canTakeShot = !!instrument?.canTakeShot && status === 'connected';
    if (status === 'idle' || status === 'error' || status === 'unsupported') survey.pending = null;
  },
  frame(hex, from) {
    survey.frames = [...survey.frames, `${from.length > 8 ? from.slice(0, 8) : from}  ${hex}`].slice(-300);
  },
  reading: onReading,
};

/** Connects to an instrument. `simulator` needs no hardware; `cave` is the open protocol; `capture` logs a device's frames. */
export async function connect(kind: LinkKind, services: string[] = []) {
  instrument?.disconnect();
  survey.message = '';
  survey.frames = [];
  if (kind === 'simulator') {
    // Its script follows the surveyor's choice: backsights if they are on, no splays (they flip to splay and fire more).
    instrument = new SimulatedInstrument(events, { steps: traverse({ seed: 7, legs: 60, backsights: survey.state.backsights, splays: 0 }) });
  } else {
    instrument = new WebBleInstrument(events, { mode: kind, services });
  }
  await instrument.connect();
}

export function disconnect() {
  instrument?.disconnect();
  instrument = null;
  survey.status = 'idle';
  survey.canTakeShot = false;
}

/** Fires a shot from the screen (the instrument must support it). */
export function takeShot() {
  instrument?.takeShot();
}

/** The simulator runs on its own, a shot every `ms`, until its script is out. */
export function autoRun(ms: number | null) {
  if (!(instrument instanceof SimulatedInstrument)) return;
  if (ms === null) instrument.stop();
  else instrument.auto(ms);
}

export const simulating = () => instrument instanceof SimulatedInstrument;
