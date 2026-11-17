// Gimbal control for an X1 riding an Osmo gimbal, laser out of the clamp's top. CaveCAD asks for laser DIRECTIONS
// (azimuth from the gimbal's front, inclination); the kinematics in ./instrument/gimbal-ik turn them into pan and roll,
// and this file moves the gimbal there and checks it with the X1's own reading. Measured behaviour behind every number:
// tools/cave-osmo/FINDINGS.md.
//
// The gimbal reports no angles and its magnet spoils the X1's compass, so pan is dead-reckoned (timed turns land within
// a few per cent) and roll is checked against each shot's inclination. Roll only reaches ~130° one way with the X1
// aboard (the "long side", which depends on how the clamp got upright) and ~40° the other; past that it snaps over, so
// the kinematics keep inside that reach and say which directions are out of it.
import { OsmoGimbal } from './instrument/osmo';
import { OM7_REACH, solve, plan, ringDirections, domeDirections, cost, type Direction, type Joints, type Reach } from './instrument/gimbal-ik';
import { survey, laser, takeShot, startDrivenSweep, finishSweep } from './survey.svelte';

/** Turn rate for planned moves (°/s): measured accurate (FINDINGS: timed turns land within ~3 %). */
const RATE = 30;
/** Settle after a move before measuring (ms). */
const SETTLE_MS = 300;
/** Average seconds a shot (1.5 when the X1 is quick, ~3.4 when slow). */
export const SHOT_S = 2.0;
/** Wait for a shot before giving up on it (ms). */
const SHOT_WAIT_MS = 4500;
/** Tilt reach from level (°), inside the measured +34 / −24. */
const TILT = { up: 30, down: 20 };
/** The deepest a ring goes below level (°). */
const RING_LOWEST = -40;

export const gimbal = new OsmoGimbal();

const LONG_KEY = 'cavecad.gimbal.longRoll';
const SIGN_KEY = 'cavecad.gimbal.rollSign';
const FACING_KEY = 'cavecad.gimbal.facing';
const load = (k: string) => {
  try {
    return localStorage.getItem(k);
  } catch {
    return null;
  }
};
const keep = (k: string, v: string) => {
  try {
    localStorage.setItem(k, v);
  } catch {
    /* not kept: fine */
  }
};

export const dome = $state({
  connected: false,
  name: '',
  /** Spacing of the shots (°). */
  step: 10,
  /** How the gimbal was set down: its front toward the forward station, or toward a wall (across the passage). */
  facing: (load(FACING_KEY) === 'wall' ? 'wall' : 'forward') as 'forward' | 'wall',
  running: false,
  phase: '',
  done: 0,
  total: 0,
  missed: 0,
  error: '',
});
gimbal.onChange = () => {
  // a fresh connection starts from the centre (set it down upright, front forward)
  if (gimbal.connected && !dome.connected) aimCentred();
  dome.connected = gimbal.connected;
  dome.name = gimbal.name;
};
export function setFacing(f: 'forward' | 'wall') {
  dome.facing = f;
  keep(FACING_KEY, f);
}
/** Where the passage runs, from the gimbal's front. */
const passageAz = () => (dome.facing === 'forward' ? 0 : -90);

export const AIM_SPEEDS = { fine: 3, normal: 10, fast: 30 } as const;
export const aim = $state({
  speed: 'normal' as keyof typeof AIM_SPEEDS,
  moving: false,
  /** Where the gimbal is thought to point, from its centre (°): turns integrated as they are sent, roll corrected by
   * each X1 shot. Reset by `aimCentred`, `aimReset`, and the end of a scan (which goes home). */
  pan: 0,
  tilt: 0,
  roll: 0,
  /** Which roll sign is the long side (1 / −1), kept on this device once seen; 0 = not known yet. */
  longRoll: Number(load(LONG_KEY)) === 1 ? 1 : Number(load(LONG_KEY)) === -1 ? -1 : 0,
  atLimit: '',
  resetting: false,
});
function setLongRoll(s: number) {
  if (s !== 1 && s !== -1) return;
  aim.longRoll = s;
  keep(LONG_KEY, String(s));
}
const reach = (): Reach => ({ ...OM7_REACH, longRoll: aim.longRoll });
const here = (): Joints => ({ pan: aim.pan, roll: aim.roll, tilt: aim.tilt });

/** Shots and seconds a scan at this spacing takes, and how many of its directions are out of reach. */
export function estimate(step: number, kind: 'ring' | 'dome' = 'ring') {
  const dirs = kind === 'dome' ? domeDirections(step) : ringDirections(passageAz(), step, RING_LOWEST);
  const p = plan(dirs, { ...reach(), longRoll: aim.longRoll || 1 });
  let travel = 0, at: Joints = { pan: 0, roll: 0, tilt: 0 };
  for (const s of p.steps) {
    travel += cost(at, s.j);
    at = s.j;
  }
  travel += cost(at, { pan: 0, roll: 0, tilt: 0 });
  return { shots: p.steps.length, missed: p.unreachable.length, secs: p.steps.length * SHOT_S + (travel / RATE) * 0.5 };
}

export async function connectGimbal() {
  dome.error = '';
  try {
    await gimbal.connect();
  } catch (e) {
    const m = e instanceof Error ? e.message : String(e);
    if (!/cancel|chosen/i.test(m)) dome.error = m;
  }
}
export const disconnectGimbal = () => gimbal.disconnect();

// ---- the X1's shots as they arrive (raw frames, decoded as crates/cave-instrument/src/cavway.rs does)
type Shot = { incl: number; roll: number; at: number };
let lastShot: Shot | null = null;
(globalThis as { __cavecadShotTap?: (b: Uint8Array) => void }).__cavecadShotTap = (b) => {
  if (b.length < 33 || b[0] !== 0x01) return;
  const a = 360 / 65535;
  const incl = (((b[7] | (b[8] << 8)) << 16) >> 16) * a;
  lastShot = { incl, roll: (b[9] | (b[10] << 8)) * a, at: performance.now() };
  if (!dome.running && !busy && gimbal.connected) fromShot(lastShot);
};
/** The laser's sideways lean from straight up (the roll axis), signed by side (+ is the side + roll leans to: on the
 * bench a + roll put the X1's roll figure at ~270); the sine keeps only the sideways part if it also leans fore/aft. */
const sideLean = (s: Shot) => -(90 - s.incl) * Math.sin((s.roll * Math.PI) / 180);
const foreLean = (s: Shot) => (90 - s.incl) * Math.cos((s.roll * Math.PI) / 180);
/** +1 if a + roll command leans the laser the way the X1 reads as + (it did on the bench), −1 if the X1 or the clamp
 * sits the other way round; learned with the long side and kept on this device. */
let rollSign = Number(load(SIGN_KEY)) === -1 ? -1 : 1;
/** Roll in the gimbal's own terms (command sign) from a shot. */
const rollOf = (s: Shot) => rollSign * sideLean(s);
/** A shot's reading says where roll really is; a deep one also says which side is long. */
function fromShot(s: Shot) {
  const r = rollOf(s);
  aim.roll = r;
  if (Math.abs(r) > 45) setLongRoll(Math.sign(r));
}

const pause = (ms: number) => new Promise((r) => setTimeout(r, ms));

async function waitShot(since: number): Promise<Shot | null> {
  const t0 = performance.now();
  while (performance.now() - t0 < SHOT_WAIT_MS) {
    if (lastShot && lastShot.at > since) return lastShot;
    await pause(80);
  }
  return null;
}

/** Measure with the laser already on (it warmed up during the move); one retry. */
async function shoot(): Promise<Shot | null> {
  for (let attempt = 0; attempt < 2; attempt++) {
    const since = performance.now();
    if (attempt) laser(true), await pause(400);
    takeShot();
    const s = await waitShot(since);
    if (s) return s;
  }
  return null;
}

/** Moves pan and roll together (each at its share of RATE, so both finish at once) and tilt to `j`; the estimate
 * follows. */
async function moveTo(j: Joints) {
  const dp = j.pan - aim.pan, dr = j.roll - aim.roll;
  const secs = Math.max(Math.abs(dp), Math.abs(dr)) / RATE;
  if (secs > 0.03) await gimbal.turn(dp / secs, 0, dr / secs, secs * 1000);
  aim.pan = j.pan;
  aim.roll = j.roll;
  const dt = j.tilt - aim.tilt;
  if (Math.abs(dt) > 0.5) await gimbal.turn(0, Math.sign(dt) * 10, 0, (Math.abs(dt) / 10) * 1000);
  aim.tilt = j.tilt;
  await pause(SETTLE_MS);
}

/** Points the laser at `d` (from the gimbal's front) and shoots: solve, move (laser warming meanwhile), shoot, and if
 * the X1 says roll fell short or overshot, correct once and shoot again. Null when out of reach or no shot. */
export async function aimAt(d: Direction): Promise<Shot | null> {
  const j = solve(d, here(), reach());
  if (!j) return null;
  laser(true);
  await moveTo(j);
  let s = await shoot();
  if (!s) return null;
  const actual = rollOf(s);
  if (Math.abs(j.roll) > 20 && Math.sign(actual) !== Math.sign(j.roll) && Math.abs(actual) > 10) {
    // snapped over the top: that side is the short one
    setLongRoll(-Math.sign(j.roll));
    aim.roll = actual;
    return s;
  }
  const off = j.roll - actual;
  aim.roll = actual;
  if (Math.abs(off) > 2.5 && Math.abs(off) < 40) {
    laser(true);
    await moveTo(j); // from where the X1 says roll really is, back to the target
    const s2 = await shoot();
    if (s2) {
      aim.roll = rollOf(s2);
      s = s2;
    }
  }
  return s;
}

/** Finds which roll side is long, and which way the X1 reads a + roll, if not known: lean 45° the + way; if it holds,
 * + is long and its reading gives the sign; if it snaps over, come back and try 45° the − way. */
async function learnLongSide() {
  if (aim.longRoll) return;
  dome.phase = 'Finding the long roll side';
  for (const cmd of [1, -1]) {
    laser(true);
    await gimbal.turn(0, 0, cmd * RATE, (45 / RATE) * 1000);
    await pause(SETTLE_MS);
    const s = await shoot();
    if (!s) return;
    const r = sideLean(s);
    if (Math.abs(r) > 35) {
      rollSign = cmd * Math.sign(r);
      keep(SIGN_KEY, String(rollSign));
      setLongRoll(cmd);
      aim.roll = cmd * Math.abs(r);
      return;
    }
    // snapped over: bring it back up by the reading before trying the other way
    await gimbal.turn(0, 0, -cmd * RATE, (45 / RATE) * 1000);
    await pause(SETTLE_MS);
  }
}

/** The Find long side button: learn it now (two 45° leans and a shot each), then home. */
export async function findLongSide() {
  if (!gimbal.connected || dome.running || busy || !survey.canTakeShot) return;
  busy = true;
  aimRelease();
  try {
    await learnLongSide();
    dome.phase = aim.longRoll ? `Long roll side found: ${aim.longRoll > 0 ? 'Roll ⟳' : '⟲ Roll'}` : 'Could not tell (no shot?)';
    await moveTo({ pan: aim.pan, roll: 0, tilt: aim.tilt });
    laser(false);
  } finally {
    busy = false;
  }
}
/** Roll reach each way now (°), for the status line. */
export const rollReach = () => {
  const r = reach();
  const m = (sgn: number) => (r.longRoll === 0 ? r.rollShort : sgn === r.longRoll ? r.rollLong : r.rollShort);
  return { left: m(-1), right: m(1) };
};

// ---- manual aiming (Nathan, 2026-11-17): the stick moves the laser forward/back (tilt; push up = forward) and turns it
// (pan); two buttons lean it left/right (roll). The guard keeps every axis inside the reach.
/** Which tilt command sign takes the laser (out of the clamp's top) forward: camera nose down, as on phone gimbals. */
const TILT_FORWARD = -1;
let stick = { x: 0, y: 0 };
let rollWant = 0;
let aimTimer: ReturnType<typeof setInterval> | undefined;
let lastTick = 0;
let sent = { pan: 0, tilt: 0, roll: 0 };
let busy = false;

function aimTick() {
  const now = performance.now();
  const dt = Math.min(0.3, (now - lastTick) / 1000);
  lastTick = now;
  aim.pan += sent.pan * dt;
  aim.tilt += sent.tilt * dt;
  aim.roll += sent.roll * dt;
  aim.atLimit = '';
  const max = AIM_SPEEDS[aim.speed];
  const shape = (v: number) => Math.sign(v) * v * v * max;
  const turnRate = shape(stick.x);
  const tiltRate = TILT_FORWARD * shape(stick.y);
  const roll = rollWant;
  const look = 0.25; // stop a quarter second early: the gimbal coasts a little
  const r = reach();
  const rollMax = (sgn: number) => (r.longRoll === 0 ? r.rollShort : sgn === r.longRoll ? r.rollLong : r.rollShort);
  const guard = (pos: number, rate: number, lo: number, hi: number, name: string) => {
    const next = pos + rate * look;
    if ((rate > 0 && next > hi) || (rate < 0 && next < -lo)) {
      aim.atLimit = name;
      return 0;
    }
    return rate;
  };
  sent = {
    pan: guard(aim.pan, turnRate, r.pan, r.pan, 'pan'),
    tilt: guard(aim.tilt, tiltRate, TILT.down, TILT.up, 'tilt'),
    roll: guard(aim.roll, roll, rollMax(-1), rollMax(1), 'roll'),
  };
  gimbal.setRates(sent.pan, sent.tilt, sent.roll);
  if (!aim.moving && !sent.pan && !sent.tilt && !sent.roll) {
    clearInterval(aimTimer);
    aimTimer = undefined;
    gimbal.release();
  }
}
function startAim() {
  aim.moving = !!(stick.x || stick.y || rollWant);
  if (!aimTimer && aim.moving) {
    lastTick = performance.now();
    aimTimer = setInterval(aimTick, 50);
    aimTick();
  }
}
/** Joystick: x (−1…1) turns, y (−1…1, up positive) moves the laser forward; squared for fine control. */
export function aimStick(x: number, y: number) {
  if (!gimbal.connected || dome.running || busy) return;
  stick = { x, y };
  startAim();
}
/** Roll buttons: lean the laser to one side or the other while held. */
export function aimHold(axis: 'roll', sign: 1 | -1) {
  if (!gimbal.connected || dome.running || busy) return;
  rollWant = AIM_SPEEDS[aim.speed] * sign;
  startAim();
}
export function aimRelease() {
  stick = { x: 0, y: 0 };
  rollWant = 0;
  aim.moving = false;
}
/** The gimbal has been set back to its centre by hand (upright, front forward): the estimate starts again from 0. */
export function aimCentred() {
  aim.pan = aim.tilt = aim.roll = 0;
  aim.atLimit = '';
}

/** Reset: back to the centre (upright, front forward). No gimbal command does this (FINDINGS: 0x4C and 0x0D tried), so
 * CaveCAD does: pan and tilt back by the estimate, then lean out by the X1's reading (sideways by roll, fore/aft by
 * tilt, 90 % a round, learning each axis's direction; up to six rounds). */
export async function aimReset() {
  if (!gimbal.connected || dome.running || busy) return;
  busy = aim.resetting = true;
  aimRelease();
  await pause(150);
  try {
    await moveTo({ pan: 0, roll: aim.roll, tilt: 0 });
    if (survey.canTakeShot) {
      const way = { side: 1, fore: 1 }; // laser change per + command, by axis
      laser(true);
      let s = await shoot();
      for (let i = 0; s && i < 6; i++) {
        const l = { side: sideLean(s), fore: foreLean(s) };
        if (Math.abs(l.side) < 2.5 && Math.abs(l.fore) < 2.5) break;
        const axis = Math.abs(l.side) >= Math.abs(l.fore) ? 'side' : 'fore';
        const off = l[axis], cmd = -Math.sign(off) * way[axis], deg = Math.abs(off) * 0.9;
        laser(true);
        if (axis === 'side') await gimbal.turn(0, 0, cmd * rollSign * RATE, (deg / RATE) * 1000);
        else await gimbal.turn(0, cmd * 10, 0, (deg / 10) * 1000);
        await pause(SETTLE_MS);
        const n = await shoot();
        if (!n) break;
        const l2 = axis === 'side' ? sideLean(n) : foreLean(n);
        if (Math.abs(l2 - off) > 2 && Math.sign(l2 - off) !== cmd) way[axis] = -way[axis];
        s = n;
      }
    } else if (Math.abs(aim.roll) > 1) {
      await moveTo({ pan: 0, roll: 0, tilt: 0 });
    }
    laser(false);
    aimCentred();
  } finally {
    busy = aim.resetting = false;
  }
}

// ---- scans: lists of directions, planned by the kinematics
let stopRequested = false;
export function stopDome() {
  stopRequested = true;
}
/** Ring scan: the cross-section square to the passage, both walls from straight up to 40° below level. */
export const startRing = () => startScan('ring');
/** Dome scan: the half-sphere above the horizon, every `step` of azimuth and inclination that the gimbal can reach. */
export const startDome = () => startScan('dome');

async function startScan(kind: 'ring' | 'dome') {
  if (dome.running || busy || !gimbal.connected || !survey.canTakeShot) return;
  const step = Math.max(2, Math.min(30, dome.step));
  aimRelease();
  dome.running = true;
  dome.error = '';
  dome.done = 0;
  dome.missed = 0;
  stopRequested = false;
  if (!startDrivenSweep('lrud')) {
    dome.running = false;
    dome.error = 'Could not start a sweep here (is a station standing?)';
    return;
  }
  try {
    await learnLongSide();
    const dirs = kind === 'dome' ? domeDirections(step) : ringDirections(passageAz(), step, RING_LOWEST);
    const p = plan(dirs, reach(), here());
    dome.total = p.steps.length;
    dome.missed = p.unreachable.length;
    for (const [i, s] of p.steps.entries()) {
      if (stopRequested) break;
      dome.phase = `${kind === 'dome' ? 'Dome' : 'Ring'} ${i + 1}/${p.steps.length}: az ${Math.round(s.d.az)}°, ${Math.round(s.d.inc)}° up`;
      // the long side may have been learned by a snap since planning: skip what is now out of reach
      if (!solve(s.d, here(), reach())) {
        dome.missed++;
        continue;
      }
      if (await aimAt(s.d)) dome.done++;
    }
    dome.phase = 'Home';
    await moveTo({ pan: 0, roll: 0, tilt: 0 });
  } catch (e) {
    dome.error = e instanceof Error ? e.message : String(e);
  } finally {
    await gimbal.stop().catch(() => {});
    laser(false);
    finishSweep();
    dome.running = false;
    dome.phase = stopRequested ? 'Stopped' : `Done: ${dome.done} shots${dome.missed ? `, ${dome.missed} out of reach` : ''}`;
  }
}
