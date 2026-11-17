// Ring and dome scans: the X1 rides an Osmo gimbal (laser out of the clamp's top) and CaveCAD turns it and fires it, so a
// station's cross-section ring is shot evenly with no hand wobble. The shots are an ordinary sweep (live check, splays,
// 3D), taken at the standing station. Measured behaviour behind every number here: tools/cave-osmo/FINDINGS.md.
//
// The gimbal reports no angles and its magnet spoils the X1's compass, so moves are timed and the X1's own reading of
// each shot (inclination, and its roll figure for which side of vertical the laser is on) steers the ring: which way
// is the long roll side, how deep each side got, how far to roll back. Roll (the ring axis) only reaches ~130° one
// way with the X1 aboard, and which way depends on how the clamp got upright, so the ring is two half-rings with a 180°
// pan between them. Set the gimbal upright (rotate button) with its centre facing a wall before starting.
import { OsmoGimbal } from './instrument/osmo';
import { survey, laser, takeShot, startDrivenSweep, finishSweep } from './survey.svelte';

/** Turn rate for every move (°/s): measured accurate (FINDINGS: timed turns land within ~3 %). */
const RATE = 30;
/** Small moves run a little long (10° asked → 11.5°); time per degree asked is scaled by this. */
const SMALL_GAIN = 0.87;
/** Deepest roll from straight up (°): with the X1 aboard the long side snaps back past ~130. */
export const DEEPEST = 120;
/** Settle after a move before measuring (s). */
const SETTLE_MS = 300;
/** Average seconds a shot (1.5 when the X1 is quick, ~3.4 when slow). */
export const SHOT_S = 2.0;
/** Pans: −90, +180, −90 at RATE. */
const PAN_S = 12;
/** Wait for a shot before giving up on it (ms). */
const SHOT_WAIT_MS = 4500;

export const gimbal = new OsmoGimbal();

export const dome = $state({
  connected: false,
  name: '',
  /** Spacing of the shots round the ring (°). */
  step: 10,
  running: false,
  phase: '',
  done: 0,
  total: 0,
  error: '',
});
gimbal.onChange = () => {
  dome.connected = gimbal.connected;
  dome.name = gimbal.name;
};

/** How far the dome pans each way from the gimbal's centre (its stops are ~±143°): the ~90° behind is unreached. */
export const DOME_PAN = 135;

/** Shots and seconds a scan at this spacing takes. Ring: a cross-section across the passage. Dome: the hemisphere
 * above the horizon, as meridians from straight up to level every `step` of pan. */
export function estimate(step: number, kind: 'ring' | 'dome' = 'ring') {
  if (kind === 'dome') {
    const meridians = Math.floor((2 * DOME_PAN) / step) + 1;
    const shots = meridians * (Math.floor(90 / step) + 1);
    return { shots, secs: shots * SHOT_S + (2 * DOME_PAN) / RATE };
  }
  const perSide = Math.floor(DEEPEST / step);
  const shots = 1 + 2 * perSide;
  const roll = (4 * DEEPEST) / RATE; // down and back, both sides
  return { shots, secs: shots * SHOT_S + PAN_S + roll * 0.4 };
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

// ---- manual aiming: a joystick (pan, roll) and tilt buttons, kept inside the gimbal's safe reach
export const AIM_SPEEDS = { fine: 3, normal: 10, fast: 30 } as const;
/** Safe reach from the gimbal's centre (°), inside the measured stops (FINDINGS: pan ~±143, tilt +34/−24, roll ~130 on
 * the long side with the X1 aboard and only ~40 on the short one; past a stop it swings wildly). */
const LIMIT = { pan: 130, tiltUp: 30, tiltDown: 20, rollShort: 35, rollLong: 120 };
export const aim = $state({
  speed: 'normal' as keyof typeof AIM_SPEEDS,
  moving: false,
  /** Where the gimbal is thought to point, from the centre (°): turns integrated as they are sent, roll corrected by
   * each X1 shot. Reset by `aimCentred` (and after a scan, which ends back home). */
  pan: 0,
  tilt: 0,
  roll: 0,
  /** Which roll command sign is the long side (1 / −1), once a scan or a deep shot has shown it. */
  longRoll: 0,
  atLimit: '',
});
let want = { pan: 0, tilt: 0, roll: 0 };
let aimTimer: ReturnType<typeof setInterval> | undefined;
let lastTick = 0;
const rollMax = (sign: number) => (aim.longRoll === 0 ? LIMIT.rollShort : sign === aim.longRoll ? LIMIT.rollLong : LIMIT.rollShort);
function clampRate(pos: number, rate: number, lo: number, hi: number, dt: number, name: string) {
  const next = pos + rate * dt;
  if ((rate > 0 && next > hi) || (rate < 0 && next < -lo)) {
    aim.atLimit = name;
    return 0;
  }
  return rate;
}
function aimTick() {
  const now = performance.now();
  const dt = Math.min(0.3, (now - lastTick) / 1000);
  lastTick = now;
  // where it has got to at the rates last sent (the gimbal lands ~3 % short: counting full keeps a margin)
  aim.pan += sent.pan * dt;
  aim.tilt += sent.tilt * dt;
  aim.roll += sent.roll * dt;
  aim.atLimit = '';
  const look = 0.25; // stop a quarter second early: the gimbal coasts a little
  sent = {
    pan: clampRate(aim.pan, want.pan, LIMIT.pan, LIMIT.pan, look, 'pan'),
    tilt: clampRate(aim.tilt, want.tilt, LIMIT.tiltDown, LIMIT.tiltUp, look, 'tilt'),
    roll: clampRate(aim.roll, want.roll, rollMax(-1), rollMax(1), look, 'roll'),
  };
  gimbal.setRates(sent.pan, sent.tilt, sent.roll);
  if (!aim.moving && !sent.pan && !sent.tilt && !sent.roll) {
    clearInterval(aimTimer);
    aimTimer = undefined;
    gimbal.release();
  }
}
let sent = { pan: 0, tilt: 0, roll: 0 };
/** Sets the wanted turn rates (°/s) while the joystick or a button is held; the guard trims them at the limits. */
export function aimSet(pan: number, tilt: number, roll: number) {
  if (!gimbal.connected || dome.running) return;
  want = { pan, tilt, roll };
  aim.moving = !!(pan || tilt || roll);
  if (!aimTimer) {
    lastTick = performance.now();
    aimTimer = setInterval(aimTick, 50);
    aimTick();
  }
}
/** Joystick: x (−1…1) pans, y (−1…1, up positive) rolls; the push is squared for fine control near the middle. */
export function aimStick(x: number, y: number) {
  const max = AIM_SPEEDS[aim.speed];
  const shape = (v: number) => Math.sign(v) * v * v * max;
  aimSet(shape(x), want.tilt, shape(y));
}
export function aimHold(axis: 'pan' | 'roll' | 'tilt', sign: 1 | -1) {
  const r = AIM_SPEEDS[aim.speed] * sign;
  aimSet(axis === 'pan' ? r : want.pan, axis === 'tilt' ? r : 0, axis === 'roll' ? r : want.roll);
}
export function aimRelease() {
  want = { pan: 0, tilt: 0, roll: 0 };
  aim.moving = false;
}
/** The gimbal has been set back to its centre by hand (upright, front forward): the estimate starts again from 0. */
export function aimCentred() {
  aim.pan = aim.tilt = aim.roll = 0;
  aim.atLimit = '';
}
/** A manual shot's reading: correct the roll estimate, and learn the long roll side from a deep shot. */
function aimFromShot(s: Shot) {
  const depth = Math.abs(phiOf(s));
  if (Math.abs(aim.roll) > 5) {
    const sign = Math.sign(aim.roll);
    aim.roll = sign * depth;
    if (depth > 45) aim.longRoll = sign;
  }
}

// ---- the X1's shots as they arrive (raw frames, decoded as crates/cave-instrument/src/cavway.rs does)
type Shot = { incl: number; roll: number; at: number };
let lastShot: Shot | null = null;
(globalThis as { __cavecadShotTap?: (b: Uint8Array) => void }).__cavecadShotTap = (b) => {
  if (b.length < 33 || b[0] !== 0x01) return;
  const a = 360 / 65535;
  const incl = ((b[7] | (b[8] << 8)) << 16 >> 16) * a;
  lastShot = { incl, roll: (b[9] | (b[10] << 8)) * a, at: performance.now() };
  if (!dome.running && gimbal.connected) aimFromShot(lastShot);
};
/** The laser's angle from straight up, signed by which side of vertical it is (the X1's roll figure: ~270 or ~90). */
const phiOf = (s: Shot) => (s.roll > 180 ? 1 : -1) * (90 - s.incl);

let stopRequested = false;
export function stopDome() {
  stopRequested = true;
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

/** Laser on while the gimbal turns (the X1 warms up meanwhile), settle, measure, wait for the shot; one retry. */
async function moveAndShoot(yaw: number, roll: number, deg: number): Promise<Shot | null> {
  laser(true);
  if (deg > 0) await gimbal.turn(yaw, 0, roll, (deg / RATE) * 1000 * (deg <= 20 ? SMALL_GAIN : 1));
  await pause(SETTLE_MS);
  for (let attempt = 0; attempt < 2; attempt++) {
    const since = performance.now();
    if (attempt) laser(true), await pause(400);
    takeShot();
    const s = await waitShot(since);
    if (s) {
      dome.done++;
      return s;
    }
  }
  return null;
}

async function turnOnly(yaw: number, roll: number, deg: number) {
  await gimbal.turn(yaw, 0, roll, (deg / RATE) * 1000);
  await pause(SETTLE_MS);
}

/** Ring scan: one cross-section at the standing station (see ringBody). */
export const startRing = () => startScan('ring');
/** Dome scan: the hemisphere above the horizon at the standing station (see domeBody). */
export const startDome = () => startScan('dome');

/** Dome: pan to −DOME_PAN, then meridian by meridian: roll from straight up down to level on the long roll side,
 * pan one step, roll back up, pan one step, … (zig-zag, so no time is spent rolling back), then home. */
async function domeBody(step: number) {
  dome.phase = 'Turning to the start';
  await turnOnly(-RATE, 0, DOME_PAN);
  let pan = -DOME_PAN;
  const s0 = await moveAndShoot(0, 0, 0);
  let depth = s0 ? Math.abs(phiOf(s0)) : 0;
  let dir = 1, sideSign = 0, learned = false, down = true;
  while (!stopRequested) {
    dome.phase = `Pan ${pan}°: ${Math.round(depth)}° from up`;
    const atEnd = down ? depth >= 90 - step / 2 : depth <= step / 2;
    if (atEnd) {
      if (pan + step > DOME_PAN) break;
      const n = await moveAndShoot(RATE, 0, step);
      pan += step;
      if (n) depth = Math.abs(phiOf(n));
      down = !down;
      continue;
    }
    const n = await moveAndShoot(0, (down ? dir : -dir) * RATE, step);
    if (!n) {
      depth += down ? step : -step;
      continue;
    }
    const p = phiOf(n);
    if (!sideSign && Math.abs(p) > 10) sideSign = Math.sign(p);
    if (!learned && sideSign && Math.sign(p) !== sideSign && Math.abs(p) > 10) {
      // snapped over the top on the first meridian: the other way is the long roll side
      dir = -dir;
      sideSign = Math.sign(p);
    }
    if (Math.abs(p) > 45) {
      learned = true;
      aim.longRoll = dir;
    }
    depth = Math.abs(p);
  }
  dome.phase = 'Home';
  if (depth > 3) await turnOnly(0, -dir * RATE, depth);
  if (pan) await turnOnly(pan > 0 ? -RATE : RATE, 0, Math.abs(pan));
}

/** One ring at the standing station: pan −90 to look along the passage, straight up, roll down one wall in `step`
 * steps, roll back, pan 180 (same ring plane, other wall), roll down again, roll back, pan −90 home. */
async function startScan(kind: 'ring' | 'dome') {
  if (dome.running || !gimbal.connected || !survey.canTakeShot) return;
  const step = Math.max(2, Math.min(30, dome.step));
  dome.running = true;
  dome.error = '';
  dome.done = 0;
  dome.total = estimate(step, kind).shots;
  stopRequested = false;
  if (!startDrivenSweep('lrud')) {
    dome.running = false;
    dome.error = 'Could not start a sweep here (is a station standing?)';
    return;
  }
  try {
    if (kind === 'dome') {
      await domeBody(step);
      return;
    }
    dome.phase = 'Turning to look along the passage';
    await turnOnly(-RATE, 0, 90);
    dome.phase = 'Straight up';
    let s = await moveAndShoot(0, 0, 0);
    let dir = 1; // roll command sign; flipped if this way turns out to be the short side
    for (const side of [1, 2]) {
      let depth = s ? Math.abs(phiOf(s)) : 0; // how far from straight up on this side
      let sideSign = 0; // which side of vertical this half-ring is on (from the first reading past 10°)
      let flipped = side === 2; // only the first side may still learn the long direction
      let misses = 0, moved = 0; // moved: degrees rolled this side (the way back if the X1 stops answering)
      while (!stopRequested && depth < DEEPEST && moved < DEEPEST + 40) {
        dome.phase = `Wall ${side}: ${Math.round(depth)}° from up`;
        const next = await moveAndShoot(0, dir * RATE, step);
        moved += step;
        if (!next) {
          if (++misses >= 3) break;
          depth += step;
          continue;
        }
        const p = phiOf(next);
        if (!sideSign && Math.abs(p) > 10) sideSign = Math.sign(p);
        if (sideSign && Math.sign(p) !== sideSign && Math.abs(p) > 10) {
          // snapped back over the top: this way is the short roll side
          if (!flipped && depth < 80) {
            dir = -dir;
            sideSign = Math.sign(p);
            flipped = true;
            depth = Math.abs(p);
            s = next;
            continue;
          }
          break;
        }
        depth = Math.abs(p);
        if (depth > 45) aim.longRoll = dir;
        s = next;
      }
      if (stopRequested) break;
      dome.phase = 'Back to straight up';
      await turnOnly(0, -dir * RATE, depth);
      if (side === 1) {
        dome.phase = 'Turning round for the other wall';
        await turnOnly(RATE, 0, 180);
        s = await moveAndShoot(0, 0, 0);
      }
    }
    dome.phase = 'Home';
    await turnOnly(-RATE, 0, 90);
  } catch (e) {
    dome.error = e instanceof Error ? e.message : String(e);
  } finally {
    aimCentred();
    await gimbal.stop().catch(() => {});
    laser(false);
    finishSweep();
    dome.running = false;
    dome.phase = stopRequested ? 'Stopped' : `Done: ${dome.done} shots`;
  }
}
