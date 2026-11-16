// The live Survey session, as the screen sees it. The session itself (what a reading is, where the surveyor
// stands, the log, the held readings, the notebook writes) is Rust: `cave-session` and `cave-instrument`, reached
// through `engine.survey*`. Every call answers with one step: log lines for the capture section, bytes for the
// transport to write back, and a snapshot of the session. This file keeps the connection (the transport or the
// simulator), the three timers the engine cannot keep (it has no clock), and a copy of the snapshot for Svelte.

import { cave, changed, focusStation } from './cave.svelte';
import { engine, loadEngine } from './engine';
import { WebBleInstrument } from './instrument/webble';
import type { Catalog, Device, HeldView, Instrument, LinkBridge, LinkEvents, LinkStatus, LogEntry, Reading, SessionState, Snapshot, Step, StandingWalls, SurveyStandard, CalibrationView, SweepKind, SweepView } from './survey-types';

export type { LogEntry } from './survey-types';
/** `auto`: any supported instrument (Cavway X1, SAP5, SAP6, BRIC4, BRIC5), told apart by its name; `capture`: any device, frames logged; `simulator`: a pretend instrument. */
export type LinkKind = 'auto' | 'capture' | 'simulator';

/** How long after the last shot a group still waiting (one or two close shots) is settled as splays. */
const IDLE_FLUSH_MS = 8000;
/** How often the simulator shoots a station when it runs on its own. */
const RUN_EVERY_MS = 1200;
/** How often CaveCAD fires the laser during a sweep it runs itself (the X1's own scan mode manages about 1.5 a second). */
const SWEEP_EVERY_MS = 1500;
/** The X1's own scan mode stops at 200 shots: CaveCAD's firing does too. */
const SWEEP_MAX_SHOTS = 200;

const EMPTY_STATE: SessionState = { mode: 'leg', backsights: false, from: '', to: '', awaiting: null, last: null };

const STANDARD_KEY = 'cavecad.surveyStandard';
const ACCURACY_KEY = 'cavecad.accuracyCheck';
/** How touchy CaveCAD is about the X1's accuracy complaints (its own limits are tight and cannot be changed). */
export type AccuracyCheck = 'strict' | 'normal' | 'relaxed';

function loadAccuracy(): AccuracyCheck {
  try {
    const v = localStorage.getItem(ACCURACY_KEY);
    return v === 'strict' || v === 'relaxed' ? v : 'normal';
  } catch {
    return 'normal';
  }
}

function loadStandard(): SurveyStandard {
  try {
    return localStorage.getItem(STANDARD_KEY) === 'manual' ? 'manual' : 'digital';
  } catch {
    return 'digital';
  }
}

export const survey = $state({
  /** Manual (mostly sketched: LRUD needs the four walls) or Digital (the instrument captures the detail). Kept on this device. */
  standard: loadStandard(),
  /** How touchy CaveCAD is about the X1's accuracy complaints. Kept on this device. */
  accuracy: loadAccuracy(),
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
  /** The sweep being taken, with its live check, and the one finished last. */
  sweep: null as SweepView | null,
  lastSweep: null as SweepView | null,
  sweepKind: 'lrud' as SweepKind,
  /** During a sweep started here, CaveCAD fires the laser itself (otherwise the surveyor uses the X1's scan mode or its button). */
  autoFire: true,
  /** CaveCAD is firing the laser for a sweep right now. */
  firing: false,
  /** The laser can be switched from the screen (an X1). */
  canLaser: false,
  /** Digital with an X1: waiting for the surveyor to start the scan on the X1 (this kind of sweep). */
  waitingForDevice: null as SweepKind | null,
  /** A shot button's first press turned the laser on: which one ('' when not aiming); its next press fires. */
  aiming: '' as '' | ShotKind,
  /** Shots of a station-to-station burst still to fire. */
  burst: 0,
  /** The calibration guide, while it is open. */
  calibration: null as CalibrationView | null,
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
  // a shot arrived (the second press, or the X1's own button): the aim is over
  const arrived = !!s.last && JSON.stringify(s.last) !== JSON.stringify(survey.last);
  if (arrived && survey.aiming) stopAiming();
  // a station-to-station burst: each shot fires once the one before it has come back
  if (arrived && survey.burst > 0) queueMicrotask(nextInBurst);
  // a Single shot is a splay: no need to wait to see whether two more make a leg
  if (arrived && singlePending) {
    singlePending = false;
    if (s.waiting > 0) queueMicrotask(settleNow);
  }
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
  survey.sweep = s.sweep ?? null;
  survey.lastSweep = s.last_sweep ?? null;
  survey.sweepKind = s.sweep_kind ?? survey.sweepKind;
  // the switch is kept on this device: the engine (fresh on every load, and deaf until a cave is open) follows it
  if (s.standard && s.standard !== survey.standard) queueMicrotask(() => apply(engine.surveySetStandard(survey.standard)));
  // CaveCAD's own firing stops once the sweep has what it needs (the surveyor can still add shots, or Finish)
  if (survey.firing && survey.sweep && (survey.sweep.check.complete || survey.sweep.check.shots >= SWEEP_MAX_SHOTS)) setFiring(false);
  watchDeviceSweep();
}

// ---- a sweep run on the X1 itself (its scan mode) ----
/** The X1 streams about 1.5 scan shots a second: this long with none means the surveyor ended the scan on the X1. */
const DEVICE_SWEEP_IDLE_MS = 8000;
let deviceIdle: ReturnType<typeof setTimeout> | undefined;
let lastDeviceShots = 0;

/**
 * Follows a sweep the X1 is running in its own scan mode: the first scan shot ends the wait, and the sweep is
 * finished here when the X1's scan ends: its next ordinary shot (the engine closes the sweep then), the scan
 * stream going quiet, or the X1's 200-shot limit.
 */
function watchDeviceSweep() {
  const w = survey.sweep;
  if (w && !w.armed) {
    survey.waitingForDevice = null;
    if (w.check.shots !== lastDeviceShots) {
      lastDeviceShots = w.check.shots;
      clearTimeout(deviceIdle);
      if (w.check.shots >= SWEEP_MAX_SHOTS) queueMicrotask(finishSweep);
      else deviceIdle = setTimeout(finishSweep, DEVICE_SWEEP_IDLE_MS);
    }
  } else if (!w) {
    clearTimeout(deviceIdle);
    lastDeviceShots = 0;
  }
}

/** Takes in what the engine answered: its log lines, its snapshot, the timers it wants. */
export function apply(step: Step): boolean {
  for (const f of step.frames) survey.frames = [...survey.frames, `${f.from.length > 8 ? f.from.slice(0, 8) : f.from}  ${f.hex}`].slice(-300);
  if (step.error) survey.message = step.error;
  if ('calibration' in step) survey.calibration = step.calibration ?? null;
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
  // the Notebook's rule, here too: a phone shows the Capture panel without the Notebook ever having opened to choose
  if (cave.hasCave && !engine.notebookTrips().some((t) => t.id === cave.trip)) cave.trip = engine.notebookTrips()[0]?.id ?? '';
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
    survey.canLaser = !!instrument?.canLaser && status === 'connected';
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
  engine.surveySetAccuracy(survey.accuracy); // the link made on connect takes it
  instrument = new WebBleInstrument(events, bridge, { mode: kind === 'capture' ? 'capture' : 'auto', services: options.services, anyDevice: options.anyDevice });
  await instrument.connect();
}

export function disconnect() {
  stopAiming();
  survey.burst = 0;
  clearTimeout(burstTimer);
  setFiring(false);
  clearTimeout(idleTimer);
  clearTimeout(settleTimer);
  setRunning(false);
  instrument?.disconnect();
  instrument = null;
  if (survey.kind) apply(engine.surveyDisconnect());
  simulator = false;
  survey.status = 'idle';
  survey.canTakeShot = false;
  survey.canLaser = false;
  survey.kind = '';
  survey.simNote = '';
}

/** Fires a shot from the screen (the instrument must support it). */
/** How long the laser stays on for aiming before the button forgets the first press. */
const AIM_MS = 60_000;
let aimTimer: ReturnType<typeof setTimeout> | undefined;

function stopAiming() {
  survey.aiming = '';
  clearTimeout(aimTimer);
}

/** Single shot (one shot: a splay, or a wall in LRUD) or station to station (three close shots: the leg). */
export type ShotKind = 'single' | 'leg';
/** The X1 makes a leg of three close shots. */
const LEG_SHOTS = 3;
/** Pause between the shots of a burst, and how long one may take to come back before the burst gives up. */
const BURST_GAP_MS = 250;
const BURST_WAIT_MS = 5000;
let burstTimer: ReturnType<typeof setTimeout> | undefined;
/** A Single shot was fired and has not come back yet. */
let singlePending = false;

function fireInBurst() {
  clearTimeout(burstTimer);
  burstTimer = setTimeout(() => {
    if (survey.burst > 0) {
      survey.burst = 0;
      survey.message = 'A shot of the leg did not come back: hold steady and press Station to station again.';
    }
  }, BURST_WAIT_MS);
  takeShot();
}

function nextInBurst() {
  if (survey.burst <= 0) return;
  survey.burst -= 1;
  clearTimeout(burstTimer);
  if (survey.burst === 0) return; // the last one is in
  setTimeout(fireInBurst, BURST_GAP_MS);
}

/** The instrument's laser can be turned on from here, so a shot is aimed first (an X1, or the simulator being one). */
export const aimsFirst = () => survey.canLaser || (survey.kind === 'simulator' && survey.status === 'connected' && survey.sim.device === 'x1'); // read from state, so the screen follows it

/**
 * The shot button: with an X1, the first press turns the laser on to aim, the second takes the shot (as on the
 * instrument). Other instruments, and CaveCAD's own sweep firing, shoot at once.
 */
export function pressShot(kind: ShotKind = 'single') {
  if (!aimsFirst() || survey.firing) return takeShot();
  if (survey.burst > 0) return; // a leg is being shot
  if (survey.aiming !== kind) {
    laser(true);
    survey.aiming = kind;
    clearTimeout(aimTimer);
    aimTimer = setTimeout(stopAiming, AIM_MS);
    return;
  }
  stopAiming();
  if (kind === 'single') {
    singlePending = true;
    if (simulator) return void apply(engine.surveySimShot());
    return takeShot();
  }
  // station to station: the simulator shoots its whole next station; a real X1 three close shots in a row
  if (simulator) return takeShot();
  survey.burst = LEG_SHOTS;
  fireInBurst();
}

export function takeShot() {
  // The simulator shoots a whole station (leg, backsight, splays and passage) for one press; a real instrument fires one shot.
  // the simulated surveyor aims a Manual LRUD shot at the wall asked for, and sweeps round otherwise
  const aim = survey.standard === 'manual' ? (survey.sweep?.check.aim?.angle ?? -1) : -1;
  const sweeping = survey.sweep?.check.kind ?? survey.waitingForDevice;
  if (simulator) apply(sweeping ? engine.surveySimSweepShot(sweeping, aim) : engine.surveySimStation(survey.randomPassage));
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
export const previousStation = () => void apply(engine.surveyPreviousStation());
export const setNames = (from: string, to: string) => void apply(engine.surveySetNames(from, to));
export const accept = () => void apply(engine.surveyAccept());
export const discard = () => void apply(engine.surveyDiscard());
/** Settle what is waiting as splays now, without waiting for the pause. */
export const settleNow = () => void apply(engine.surveySettleNow());
/** Takes back the last thing written: the shot is removed (or its backsight cleared) and the surveyor stands where they did. */
export const undoLast = () => apply(engine.surveyUndoLast());
/** Writes one dimension at the standing station. */
export const setStandingWall = (side: 'left' | 'right' | 'up' | 'down', value: number | null) => apply(engine.surveySetStandingWall(side, value));

// ---- the instrument's modes ----

/** Switches the X1's laser on or off (it stays on, for aiming, until switched off or a shot is taken). */
export function laser(on: boolean) {
  if (!on) stopAiming();
  instrument?.laser?.(on);
}

let sweepTimer: ReturnType<typeof setInterval> | undefined;
let laserForSweep = false;

function setFiring(on: boolean) {
  survey.firing = on;
  clearInterval(sweepTimer);
  sweepTimer = on ? setInterval(takeShot, SWEEP_EVERY_MS) : undefined;
}

/**
 * Starts a sweep at the standing station: every shot until Finish is a sweep shot (a splay, never part of a leg),
 * checked live as `kind`. With Auto-fire on and an instrument that can be fired from here, CaveCAD fires the laser
 * every 1.5 s until the check passes; otherwise the surveyor shoots (or uses the X1's own scan mode).
 */
export function startSweep(kind: SweepKind) {
  // Digital with an X1: the sweep is the X1's own scan mode, started and ended on the X1; CaveCAD follows it
  if (survey.standard === 'digital' && usesDeviceScan()) {
    setSweepKind(kind);
    survey.waitingForDevice = kind;
    showStation(survey.standing);
    if (simulator) setFiring(true); // the simulated X1 scans as soon as asked
    return;
  }
  if (!apply(engine.surveyStartSweep(kind))) return;
  if (survey.sweep) showStation(survey.sweep.station);
  laserForSweep = false;
  // Manual is four aimed shots (or a rough outline): the surveyor fires each one; Digital can fire on its own
  if (survey.standard === 'digital' && survey.autoFire && survey.canTakeShot) {
    if (survey.canLaser) {
      laser(true);
      laserForSweep = true;
    }
    setFiring(true);
  }
}

/** Pauses or carries on CaveCAD's firing during a sweep. */
export function toggleFiring() {
  if (survey.sweep && survey.canTakeShot) setFiring(!survey.firing);
}

/** Ends the sweep under way (kept, with its check, as the last sweep). */
export function finishSweep() {
  setFiring(false);
  survey.waitingForDevice = null;
  clearTimeout(deviceIdle);
  if (laserForSweep) laser(false);
  laserForSweep = false;
  apply(engine.surveyFinishSweep());
}

/** What a sweep the instrument starts on its own (the X1's scan mode) is checked as. */
export function setSweepKind(kind: SweepKind) {
  survey.sweepKind = kind;
  apply(engine.surveySetSweepKind(kind));
}

/** Brings the Map to a station and cuts the Cross Section there (a sweep's ring shows in it as it fills). */
function showStation(label: string) {
  const found = engine.sectionStations(false, true).filter((st) => st.label === label).pop();
  if (!found) return;
  cave.station = found.id;
  focusStation(found.id);
}

/** Switches between Manual (mostly sketched: an LRUD ring needs one shot on each of the four walls) and Digital (the instrument captures the detail: dense rings, full room outlines). */
export function setStandard(standard: SurveyStandard) {
  survey.standard = standard;
  try {
    localStorage.setItem(STANDARD_KEY, standard);
  } catch {
    /* kept for this visit only */
  }
  apply(engine.surveySetStandard(standard));
}

/** The instrument has a scan mode CaveCAD can follow: a Cavway X1 (or the simulator pretending to be one). */
export function usesDeviceScan(): boolean {
  return survey.status === 'connected' && (survey.canLaser || (simulator && survey.sim.device === 'x1'));
}

/** Stops waiting for the X1 to start its scan. */
export function cancelDeviceSweep() {
  setFiring(false);
  survey.waitingForDevice = null;
}

/** Sets how touchy CaveCAD is about the X1's accuracy complaints (Strict: every one the X1 makes). */
export function setAccuracy(level: AccuracyCheck) {
  survey.accuracy = level;
  try {
    localStorage.setItem(ACCURACY_KEY, level);
  } catch {
    /* kept for this visit only */
  }
  apply(engine.surveySetAccuracy(level));
}

// ---- guided calibration ----

/** Opens the calibration guide, fresh: the X1's calibration shots are followed from now on. */
export const startCalibration = () => void apply(engine.surveyCalibrationStart());
/** Closes the calibration guide. */
export const stopCalibration = () => void apply(engine.surveyCalibrationStop());
/** The group being taken was reset on the X1 (its > button). */
export const resetCalibrationGroup = () => void apply(engine.surveyCalibrationResetGroup());
/** A simulated calibration shot, aimed where the guide asks. */
export const simCalibrationShot = () => void apply(engine.surveySimCalibrationShot());
