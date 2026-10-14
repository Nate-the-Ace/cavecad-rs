// The live Survey session: ties an instrument link to the notebook. Shots come in from a link (a Cavway X1
// over Bluetooth, or the simulator); for an X1 `legs.ts` groups them into legs and splays; `survey.ts`
// decides what each is; `survey-run.ts` writes it to the notebook through the engine's existing notebook
// calls, so every shot is an ordinary document edit (undoable, merged like any other, shown on the Map,
// Profile and Cross Section the moment it lands). Nothing here is stored in the cave: the session (where the
// surveyor stands, the mode, the log) is view state rebuilt from the notebook page when a trip is opened.

import { cave, changed } from './cave.svelte';
import { engine } from './engine';
import { SIDES, withSide, type NotebookPage, type Side } from './notebook';
import { LegAssembler, type LegEvent } from './instrument/legs';
import { traverse } from './instrument/simulator';
import { X1SimulatedInstrument } from './instrument/x1sim';
import { WebBleInstrument } from './instrument/webble';
import type { Instrument, LinkEvents, LinkStatus, Reading } from './instrument/types';
import { advance, decide, decideEvent, nextStep, skipBack, standing, start, withMode, type Mode, type SessionState } from './survey';
import { run, type Entry, type Incoming } from './survey-run';
import { undo as undoWritten, type NotebookPort } from './survey-writer';

export type LogEntry = Entry;
export type LinkKind = 'x1' | 'caveble' | 'capture' | 'simulator';

/** How long after the last shot an X1 group still waiting (one or two close shots) is settled as splays. */
const IDLE_FLUSH_MS = 8000;

export const survey = $state({
  status: 'idle' as LinkStatus,
  detail: '',
  device: '',
  canTakeShot: false,
  /** The instrument sends single shots and says itself what is a leg (an X1): the Leg/Splay switch is not used. */
  groupsLegs: false,
  /** Where the surveyor stands and what they are doing. */
  state: start('', new Set()) as SessionState,
  /** Write each thing at once (default, as TopoDroid does) or hold it for an Accept. */
  auto: true,
  /** Things waiting for Accept (only when `auto` is off), oldest first. */
  held: [] as Incoming[],
  /** Close shots the X1 grouping is holding while it waits to see if they become a leg (0, 1 or 2). */
  waiting: 0,
  /** The latest reading, written or not, for the big display. */
  last: null as Reading | null,
  log: [] as Entry[],
  /** Raw frames, newest last, for working out an instrument whose protocol is not known. */
  frames: [] as string[],
  /** Why the last thing was refused (or the last edit failed). Empty when all is well. */
  message: '',
  /** Worth a look about the last thing written, which was written anyway. */
  warnings: [] as string[],
  /** The trip this session was set up for: a different trip starts it again. */
  trip: '',
});

let instrument: Instrument | null = null;
let counter = 0;
const assembler = new LegAssembler();
let flushTimer: ReturnType<typeof setTimeout> | undefined;

/** Every station name in use anywhere in the cave and on this page: never suggest one that would close a loop by accident. */
function takenNames(page: NotebookPage | null): Set<string> {
  const taken = new Set(engine.scanViewStations('map', false).map((s) => s.label));
  for (const r of page?.rows ?? []) taken.add(r.label);
  return taken;
}

const names = () => takenNames(cave.trip ? engine.notebookPage(cave.trip) : null);

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
  survey.held = [];
  survey.message = '';
  survey.warnings = [];
  assembler.reset();
  survey.waiting = 0;
}

const fail = (m: string) => {
  if (m) survey.message = m.replace(/^error: /, '');
  return false;
};

/** The notebook calls the writer needs, answered by the real engine. */
const port: NotebookPort = {
  page: (trip) => engine.notebookPage(trip),
  addShot: (trip, from, to, splay) => engine.notebookAddShot(trip, from, to, splay),
  setReading: (shot, which, value) => engine.notebookSetReading(shot, which, value),
  removeShot: (shot) => engine.notebookRemoveShot(shot),
};

/** Writes one thing to the notebook. Returns true when it was written (or an earlier leg was refined). */
export function commit(incoming: Incoming): boolean {
  const trip = cave.trip;
  if (!trip) return fail('Open a cave first.');
  const out = run(port, trip, survey.state, incoming, names, survey.log);
  if (!out.ok) return fail(out.error);
  if ('entry' in out) {
    survey.log = [...survey.log, { ...out.entry, n: ++counter }].slice(-50);
    survey.warnings = out.warnings;
  } else {
    survey.log = survey.log.map((e) => (e.shot === out.updated.shot ? out.updated : e));
  }
  survey.message = '';
  survey.state = out.state;
  changed();
  return true;
}

/** What to do with something that has arrived: write it, or hold it for Accept. */
function handle(incoming: Incoming) {
  if (survey.auto) {
    commit(incoming);
    return;
  }
  if (incoming.kind === 'leg-update') {
    // A held leg gained a shot: the held one is replaced. One already written is refined without asking.
    const i = survey.held.findIndex((h) => h.kind === 'leg' && h.group === incoming.group);
    if (i >= 0) survey.held = survey.held.map((h, k) => (k === i ? ({ ...incoming, kind: 'leg' } as Incoming) : h));
    else commit(incoming);
    return;
  }
  survey.held = [...survey.held, incoming];
  survey.message = '';
}

/** What the held thing would do when accepted, for the preview card. */
export function preview(incoming: Incoming): { text: string; warnings: string[]; reject: string | null } {
  const taken = names();
  const d = incoming.kind === 'single' ? decide(survey.state, incoming.reading) : incoming.kind === 'leg-update' ? null : decideEvent(survey.state, incoming, taken).decision;
  if (!d) return { text: 'a refined leg', warnings: [], reject: null };
  const p = d.plan;
  const what = !p ? '' : p.kind === 'leg' ? `${p.from} to ${p.to}` : p.kind === 'splay' ? `a splay at ${p.from}` : 'the backsight';
  return { text: what, warnings: d.warnings, reject: d.reject };
}

/** Writes the oldest held thing (the Accept button). */
export function accept() {
  const [next, ...rest] = survey.held;
  if (!next) return;
  survey.held = rest;
  commit(next);
}

/** Throws the oldest held thing away (the Discard button). */
export function discard() {
  survey.held = survey.held.slice(1);
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
  assembler.reset(); // so that shooting the same leg again right away is a new leg, not a refinement of the one just taken back
  changed();
  return true;
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

/** Hands what the grouping has settled to `handle`. */
function settle(events: LegEvent[]) {
  for (const e of events) handle(e);
}

/** Settle what is waiting as splays now, without waiting for the pause (the surveyor means them to be splays). */
export function settleNow() {
  clearTimeout(flushTimer);
  settle(assembler.flush());
  survey.waiting = 0;
}

/** A shot arrived from the instrument. */
function onReading(reading: Reading) {
  survey.last = reading;
  if (!instrument?.groupsLegs) {
    handle({ kind: 'single', reading });
    return;
  }
  settle(assembler.push(reading));
  survey.waiting = assembler.waiting;
  // One or two close shots may yet become a leg; when the surveyor pauses they are splays.
  clearTimeout(flushTimer);
  flushTimer = setTimeout(() => {
    settle(assembler.flush());
    survey.waiting = 0;
  }, IDLE_FLUSH_MS);
}

const events: LinkEvents = {
  status(status, detail = '') {
    survey.status = status;
    survey.detail = detail;
    survey.device = instrument && status === 'connected' ? instrument.name : survey.device;
    survey.canTakeShot = !!instrument?.canTakeShot && status === 'connected';
    if (status === 'idle' || status === 'error' || status === 'unsupported') survey.held = [];
  },
  frame(hex, from) {
    survey.frames = [...survey.frames, `${from.length > 8 ? from.slice(0, 8) : from}  ${hex}`].slice(-300);
  },
  reading: onReading,
};

/** Connects to an instrument. `x1` is the Cavway X1; `simulator` a pretend one; `caveble` the open protocol; `capture` logs any device's frames. */
export async function connect(kind: LinkKind, options: { services?: string[]; anyDevice?: boolean } = {}) {
  disconnect();
  survey.message = '';
  survey.frames = [];
  const device = kind === 'x1' || kind === 'simulator';
  if (kind === 'simulator') {
    instrument = new X1SimulatedInstrument(events, { steps: traverse({ seed: 7, legs: 30, backsights: survey.state.backsights, splays: 2 }), policy: 'third' });
  } else {
    instrument = new WebBleInstrument(events, { mode: kind, services: options.services, anyDevice: options.anyDevice });
  }
  survey.groupsLegs = device;
  // An X1 says itself what is a leg; any other instrument is told by the surveyor.
  survey.state = { ...survey.state, mode: device ? 'auto' : survey.state.mode === 'auto' ? 'leg' : survey.state.mode };
  assembler.reset();
  survey.waiting = 0;
  await instrument.connect();
}

export function disconnect() {
  clearTimeout(flushTimer);
  if (instrument?.groupsLegs) settle(assembler.flush());
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
  if (!(instrument instanceof X1SimulatedInstrument)) return;
  if (ms === null) instrument.stop();
  else instrument.auto(ms);
}

export const simulating = () => instrument instanceof X1SimulatedInstrument;
