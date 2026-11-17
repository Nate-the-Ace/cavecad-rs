// Dome scan: the X1 rides an Osmo gimbal (laser out of the clamp's top) and CaveCAD turns it and fires it, so a
// station's cross-section ring is shot evenly with no hand wobble. The shots are an ordinary sweep (live check, splays,
// 3D), taken at the standing station. Measured behaviour behind every number here: tools/cave-osmo/FINDINGS.md.
//
// Open loop for now: the gimbal reports no angles and its magnet spoils the X1's compass, so moves are timed. Roll
// (the ring axis) only reaches ~130° one way with the X1 aboard, so the ring is done as two half-rings with a 180° pan
// between them. Set the gimbal upright (rotate button) with its centre facing a wall before starting.
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

/** Shots and seconds a ring at this spacing takes. */
export function estimate(step: number) {
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

let stopRequested = false;
export function stopDome() {
  stopRequested = true;
}

const pause = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Laser on while the gimbal turns (the X1 warms up meanwhile), settle, measure, wait for the shot to land. */
async function moveAndShoot(yaw: number, roll: number, deg: number) {
  laser(true);
  if (deg > 0) await gimbal.turn(yaw, 0, roll, (deg / RATE) * 1000 * (deg <= 20 ? SMALL_GAIN : 1));
  await pause(SETTLE_MS);
  const before = survey.sweep?.check.shots ?? 0;
  takeShot();
  const t0 = performance.now();
  while ((survey.sweep?.check.shots ?? 0) <= before && performance.now() - t0 < SHOT_WAIT_MS) await pause(100);
  if ((survey.sweep?.check.shots ?? 0) > before) dome.done++;
}

async function turnOnly(yaw: number, roll: number, deg: number) {
  await gimbal.turn(yaw, 0, roll, (deg / RATE) * 1000);
  await pause(SETTLE_MS);
}

/** One ring at the standing station: pan −90 to look along the passage, straight up, roll down one wall in `step`
 * steps, roll back, pan 180 (same ring plane, other wall), roll down again, roll back, pan −90 home. */
export async function startDome() {
  if (dome.running || !gimbal.connected || !survey.canTakeShot) return;
  const step = Math.max(2, Math.min(30, dome.step));
  const perSide = Math.floor(DEEPEST / step);
  dome.running = true;
  dome.error = '';
  dome.done = 0;
  dome.total = estimate(step).shots;
  stopRequested = false;
  if (!startDrivenSweep('lrud')) {
    dome.running = false;
    dome.error = 'Could not start a sweep here (is a station standing?)';
    return;
  }
  try {
    dome.phase = 'Turning to look along the passage';
    await turnOnly(-RATE, 0, 90);
    dome.phase = 'Straight up';
    await moveAndShoot(0, 0, 0);
    for (const side of [1, 2]) {
      for (let i = 1; i <= perSide && !stopRequested; i++) {
        dome.phase = `Wall ${side}: ${i * step}° from up`;
        await moveAndShoot(0, RATE, step);
      }
      if (stopRequested) break;
      dome.phase = 'Back to straight up';
      await turnOnly(0, -RATE, perSide * step);
      if (side === 1) {
        dome.phase = 'Turning round for the other wall';
        await turnOnly(RATE, 0, 180);
      }
    }
    dome.phase = 'Home';
    await turnOnly(-RATE, 0, 90);
  } catch (e) {
    dome.error = e instanceof Error ? e.message : String(e);
  } finally {
    await gimbal.stop().catch(() => {});
    laser(false);
    finishSweep();
    dome.running = false;
    dome.phase = stopRequested ? 'Stopped' : `Done: ${dome.done} shots`;
  }
}
