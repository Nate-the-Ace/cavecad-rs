// The live Survey session, as the screen sees it. The session itself (what a reading is, where the surveyor
// stands, the log, the held readings, the notebook writes) is Rust: `cave-session` and `cave-instrument`, reached
// through `engine.survey*`. Every call answers with one step: log lines for the capture section, bytes for the
// transport to write back, and a snapshot of the session. This file keeps the connection (the transport or the
// simulator), the three timers the engine cannot keep (it has no clock), and a copy of the snapshot for Svelte.

import { cave, changed } from './cave.svelte';
import { engine, loadEngine } from './engine';
import { WebBleInstrument } from './instrument/webble';
import type { Catalog, Device, HeldView, Instrument, LinkBridge, LinkEvents, LinkStatus, LogEntry, Reading, SessionState, Snapshot, Step, StandingWalls } from './survey-types';

export type { LogEntry } from './survey-types';
/** `auto`: any supported instrument (Cavway X1, SAP5, SAP6, BRIC4, BRIC5), told apart by its name; `capture`: any device, frames logged; `simulator`: a pretend instrument. */
export type LinkKind = 'auto' | 'capture' | 'simulator';

/** How long after the last shot a group still waiting (one or two close shots) is settled as splays. */
const IDLE_FLUSH_MS = 8000;
/** How often the simulator shoots a station when it runs on its own. */
const RUN_EVERY_MS = 1200;

const EMPTY_STATE: SessionState = { mode: 'leg', backsights: false, from: '', to: '', awaiting: null, last: null };

export const survey = $state({
  status: 'idle' as LinkStatus,
  /** Which kind of link is up (or was asked for), so the ribbon can show it pressed. */
  kind: '' as LinkKind | '',
  /** The simulator is firing on its own. */
  running: false,
  /** Which device the simulator pretends to be, and which quirk it has: chosen in the Survey tab. */
  sim: { device: 'x1' as Device['kind'], quirk: 'none' },
  /** What the simulated quirk is meant to show, while simulating. */
  simNote: '',
  /** Connection choices for an instrument that is not found by default (kept in the panel's capture section). */
  services: '',
  anyDevice: false,
  detail: '',
  device: '',
  canTakeShot: false,
  /** The simulator's wall shots (LRUD) are taken. */
  randomPassage: true,
  /** The instruments and simulated devices the engine knows. */
  catalog: null as Catalog | null,
  /** Raw frames, newest last, for working out an instrument whose protocol is not known. */
  frames: [] as string[],
  // ---- the engine's snapshot of the session ----
  state: EMPTY_STATE,
  instruction: '',
  standing: '',
  auto: true,
  groupsLegs: false,
  held: [] as HeldView[],
  waiting: 0,
  last: null as Reading | null,
  log: [] as LogEntry[],
  message: '',
  warnings: [] as string[],
  standingWalls: null as StandingWalls | null,
});

let instrument: Instrument | null = null;
let simulator = false;
let edits = 0;
let idleTimer: ReturnType<typeof setTimeout> | undefined;
let settleTimer: ReturnType<typeof setTimeout> | undefined;
let runTimer: ReturnType<typeof setInterval> | undefined;

/** The simulator's devices and their quirks, for the Survey tab's choice. */
export const simDevices = (): Device[] => survey.catalog?.devices ?? [];

// After the module graph has loaded: the engine imports this graph too, so asking for it now would loop.
queueMicrotask(() =>
  loadEngine().then(() => {
    survey.catalog = engine.instrumentCatalog();
  }),
);

function mirror(s: Snapshot) {
  survey.state = s.state;
  survey.instruction = s.instruction;
  survey.standing = s.standing;
  survey.auto = s.auto;
  survey.groupsLegs = s.groups_legs;
  survey.held = s.held;
  survey.waiting = s.waiting;
  survey.last = s.last;
  survey.log = s.log;
  survey.message = s.message;
  survey.warnings = s.warnings;
  survey.standingWalls = s.standing_walls;
}

/** Takes in what the engine answered: its log lines, its snapshot, the timers it wants. */
export function apply(step: Step): boolean {
  for (const f of step.frames) survey.frames = [...survey.frames, `${f.from.length > 8 ? f.from.slice(0, 8) : f.from}  ${f.hex}`].slice(-300);
  if (step.error) survey.message = step.error;
  if (step.snapshot) {
    mirror(step.snapshot);
    if (step.snapshot.edits !== edits) {
      edits = step.snapshot.edits;
      changed();
    }
    // One or two close shots may yet become a leg; when the surveyor pauses they are splays.
    clearTimeout(idleTimer);
    if (step.snapshot.waiting > 0) idleTimer = setTimeout(() => apply(engine.surveySettleNow()), IDLE_FLUSH_MS);
  }
  if (simulator) {
    clearTimeout(settleTimer);
    if (step.settle_ms !== null) settleTimer = setTimeout(() => apply(engine.surveyFlushLink()), step.settle_ms);
    if (!step.sim_left && survey.running) setRunning(false);
  }
  return step.ok;
}

/**
 * Sets the session up for the open trip: standing at the last station on its page (or A1 on an empty one),
 * keeping the surveyor's mode and backsight choice. Called when the trip changes, not on every edit.
 */
export function syncToTrip() {
  if (!cave.trip) return;
  apply(engine.surveySyncTrip(cave.trip));
}

/** The engine, as the Bluetooth transport sees it. */
const bridge: LinkBridge = {
  catalog: () => survey.catalog ?? engine.instrumentCatalog(),
  identify: (name, services) => engine.instrumentIdentify(name, services),
  connect: (kind) => engine.surveyConnect(kind),
  frame: (kind, bytes) => engine.surveyFrame(kind, bytes),
  flushLink: () => engine.surveyFlushLink(),
};

const events: LinkEvents = {
  status(status, detail = '') {
    survey.status = status;
    survey.detail = detail;
    survey.device = instrument && status === 'connected' ? instrument.name : survey.device;
    survey.canTakeShot = !!instrument?.canTakeShot && status === 'connected';
    if (status === 'idle' || status === 'error' || status === 'unsupported') apply(engine.surveyLinkDown());
  },
  frame(hex, from) {
    survey.frames = [...survey.frames, `${from.length > 8 ? from.slice(0, 8) : from}  ${hex}`].slice(-300);
  },
  step(step) {
    apply(step);
  },
};

/** Connects to an instrument, a pretend one, or (capture) any device whose frames are only logged. */
export async function connect(kind: LinkKind, given: { services?: string[]; anyDevice?: boolean } = {}) {
  const options = { services: given.services ?? survey.services.split(/[\s,]+/).filter(Boolean), anyDevice: given.anyDevice ?? survey.anyDevice };
  disconnect();
  survey.frames = [];
  survey.kind = kind;
  survey.running = false;
  if (kind === 'simulator') {
    simulator = true;
    const step = engine.surveyConnect('sim', { device: survey.sim.device, quirk: survey.sim.quirk });
    survey.simNote = step.sim_note ?? '';
    instrument = { name: `Simulated ${simDevices().find((d) => d.kind === survey.sim.device)?.label ?? 'instrument'}`, canTakeShot: true, connect: async () => {}, disconnect: () => {}, takeShot: () => {} };
    apply(step);
    events.status('connected', instrument.name);
    return;
  }
  simulator = false;
  survey.simNote = '';
  survey.message = '';
  instrument = new WebBleInstrument(events, bridge, { mode: kind === 'capture' ? 'capture' : 'auto', services: options.services, anyDevice: options.anyDevice });
  await instrument.connect();
}

export function disconnect() {
  clearTimeout(idleTimer);
  clearTimeout(settleTimer);
  setRunning(false);
  instrument?.disconnect();
  instrument = null;
  if (survey.kind) apply(engine.surveyDisconnect());
  simulator = false;
  survey.status = 'idle';
  survey.canTakeShot = false;
  survey.kind = '';
  survey.simNote = '';
}

/** Fires a shot from the screen (the instrument must support it). */
export function takeShot() {
  // The simulator shoots a whole station (leg, backsight, splays and passage) for one press; a real instrument fires one shot.
  if (simulator) apply(engine.surveySimStation(survey.randomPassage));
  else instrument?.takeShot();
}

function setRunning(on: boolean) {
  survey.running = on;
  clearInterval(runTimer);
  runTimer = on ? setInterval(takeShot, RUN_EVERY_MS) : undefined;
}

/** The simulator fires on its own, or stops (the Survey tab's Run button). */
export function toggleRun() {
  if (simulator) setRunning(!survey.running);
}

export const simulating = () => simulator;

// ---- what the surveyor does ----

export const setMode = (mode: SessionState['mode']) => void apply(engine.surveySetMode(mode));
export const setBacksights = (on: boolean) => void apply(engine.surveySetBacksights(on));
export const setAuto = (auto: boolean) => void apply(engine.surveySetAuto(auto));
export const skipBacksight = () => void apply(engine.surveySkipBacksight());
export const nextStation = () => void apply(engine.surveyNextStation());
export const setNames = (from: string, to: string) => void apply(engine.surveySetNames(from, to));
export const accept = () => void apply(engine.surveyAccept());
export const discard = () => void apply(engine.surveyDiscard());
/** Settle what is waiting as splays now, without waiting for the pause. */
export const settleNow = () => void apply(engine.surveySettleNow());
/** Takes back the last thing written: the shot is removed (or its backsight cleared) and the surveyor stands where they did. */
export const undoLast = () => apply(engine.surveyUndoLast());
/** Writes one dimension at the standing station. */
export const setStandingWall = (side: 'left' | 'right' | 'up' | 'down', value: number | null) => apply(engine.surveySetStandingWall(side, value));
