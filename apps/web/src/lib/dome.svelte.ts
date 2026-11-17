// Gimbal control for an X1 riding an Osmo Mobile 7, laser out of the clamp's top. CaveCAD asks for laser DIRECTIONS
// (azimuth from the gimbal's front, inclination); ./instrument/gimbal-ik turns them into yaw and roll targets that keep
// every joint inside its safe window, and this file moves the gimbal there and trims on the gimbal's own attitude
// report (once a second). Measured behaviour behind every number: tools/cave-osmo/FINDINGS.md; the same control is
// bench-tested in the Rust library (`Osmo::aim`).
//
// The roll has no stop: past its window the clamp FLIPS OVER (violently, at any speed). So every move is guarded:
// before each step the joints are predicted from how they moved on the step before, and a step that would leave the
// window is cut short. Targets past the flip are reached by panning round and rolling the long way.
//
// The gimbal is set down facing the forward station. Its front is wherever the pan centre faces, read off the joints,
// so it does not matter where it was left pointing.
import { OsmoGimbal } from './instrument/osmo';
import {
  OM7_PORTRAIT,
  OM7_UPRIGHT_ROLL,
  solutions,
  plan,
  compile,
  programSeconds,
  ringDirections,
  domeDirections,
  homeFrom,
  pointing,
  jointsOf,
  along,
  euler,
  fromEuler,
  wrap180,
  type Att,
  type Axis,
  type Direction,
  type Op,
} from './instrument/gimbal-ik';
import { survey, laser, takeShot, startDrivenSweep, finishSweep } from './survey.svelte';

/** Turn rate for planned moves (°/s). */
const RATE = 30;
/** Trim rate (°/s). */
const TRIM_RATE = 15;
/** Seconds a shot takes with the laser warmed during the move (bench: 1.46). */
export const SHOT_S = 1.5;
/** Still time after a move before the shot (s). */
const SETTLE_S = 0.3;
/** Detail levels: spacing of the shots (°). */
export const LEVELS = [
  { name: 'Coarse', step: 30 },
  { name: 'Medium', step: 15 },
  { name: 'Fine', step: 10 },
  { name: 'Detailed', step: 5 },
] as const;
/** Wait for a shot before giving up on it (ms). */
const SHOT_WAIT_MS = 4500;
/** The deepest a ring asks for below level (°); the roll window ends ~19° below on either wall. */
const RING_LOWEST = -20;
/** Close enough (°). */
const TOL = 1.5;
const LIMITS = OM7_PORTRAIT;

export const gimbal = new OsmoGimbal();

export const dome = $state({
  connected: false,
  name: '',
  /** Which detail level (index into LEVELS). */
  level: ((): number => {
    try {
      const v = Number(localStorage.getItem('cavecad.gimbal.level'));
      return v >= 0 && v < 4 ? v : 1;
    } catch {
      return 1;
    }
  })(),
  /** Shots taken in the running (or last) scan, by direction key, and the one being taken. */
  taken: [] as string[],
  current: '',
  /** Which scan the Start button runs. */
  kind: ((): 'ring' | 'dome' => {
    try {
      return localStorage.getItem('cavecad.gimbal.scan') === 'dome' ? 'dome' : 'ring';
    } catch {
      return 'ring';
    }
  })(),
  running: false,
  phase: '',
  done: 0,
  total: 0,
  missed: 0,
  error: '',
});
gimbal.onChange = () => {
  dome.connected = gimbal.connected;
  dome.name = gimbal.name;
};

export const AIM_SPEEDS = { fine: 3, normal: 10, fast: 30 } as const;
export const aim = $state({
  speed: 'normal' as keyof typeof AIM_SPEEDS,
  moving: false,
  /** Where the clamp top points from the gimbal's front, and the joints, from the last report. */
  az: 0,
  inc: 90,
  joints: [0, 0, 0] as [number, number, number],
  /** Has the gimbal reported yet (answering its handshake starts the reports). */
  reporting: false,
  atLimit: '',
  resetting: false,
});

const pause = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** The front-facing, upright home for the latest report, or why there is none. */
function homeNow(): { home: Att | null; why: string } {
  const a = gimbal.attitude;
  if (!a) return { home: null, why: 'Waiting for the gimbal to report its position' };
  const home = homeFrom(a);
  if (home.roll < OM7_UPRIGHT_ROLL[0] || home.roll > OM7_UPRIGHT_ROLL[1])
    return { home: null, why: 'The clamp is upright the other way round: double-press the rotate button to turn it back, then try again' };
  return { home, why: '' };
}

gimbal.onPosition = () => {
  const a = gimbal.attitude;
  if (!a) return;
  aim.reporting = true;
  aim.joints = jointsOf(a);
  const { home } = homeNow();
  if (home) {
    const p = pointing(a, home);
    aim.az = p.az;
    aim.inc = p.inc;
  }
};

/** A home for planning when the gimbal has not reported yet (estimates only). */
const NOMINAL_HOME: Att = { q: fromEuler(-90, 0, 0), pan: 0, tilt: 25, roll: 95 };

/** A direction's key, for marking progress. */
export const dirKey = (d: Direction) => `${Math.round(d.az * 10)},${Math.round(d.inc * 10)}`;

type Program = { ops: Op[]; steps: ReturnType<typeof plan>['steps']; points: (Direction & { ok: boolean; key: string })[]; shots: number; missed: number; secs: number };
// worked out once per scan, level and home (rounded: the report wobbles a little), not while it runs
const programs = new Map<string, Program>();
function program(kind: 'ring' | 'dome', level: number): Program {
  const home = homeNow().home ?? NOMINAL_HOME;
  const [r, p, y] = euler(home.q);
  const key = `${kind}:${level}:${Math.round(home.roll / 2)}:${Math.round(home.tilt / 2)}:${Math.round(r / 2)}:${Math.round(p / 2)}`;
  let got = programs.get(key);
  if (!got) {
    const step = LEVELS[level].step;
    const dirs = kind === 'dome' ? domeDirections(step) : ringDirections(0, step, RING_LOWEST);
    // the plan and moves are made relative to home's heading; the heading itself only rotates them, so leave it out
    const h: Att = { ...home, q: fromEuler(r, p, 0) };
    const pl = plan(dirs, h);
    const ops = compile(pl, h, RATE);
    const miss = new Set(pl.unreachable);
    got = {
      ops,
      steps: pl.steps,
      points: dirs.map((d) => ({ ...d, ok: !miss.has(d), key: dirKey(d) })),
      shots: pl.steps.length,
      missed: pl.unreachable.length,
      secs: programSeconds(ops, { shotS: SHOT_S, settleS: SETTLE_S }),
    };
    if (programs.size > 40) programs.clear();
    programs.set(key, got);
    void y;
  }
  return got;
}

/** Shots, seconds and the picture's points for a scan at a detail level (precalculated, cached). */
export const estimate = (level: number, kind: 'ring' | 'dome') => program(kind, level);

export function setLevel(i: number) {
  dome.level = i;
  try {
    localStorage.setItem('cavecad.gimbal.level', String(i));
  } catch {
    /* not kept: fine */
  }
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

// ---- the X1's shots as they arrive (raw frames): only to know a shot came back
let lastShotAt = 0;
(globalThis as { __cavecadShotTap?: (b: Uint8Array) => void }).__cavecadShotTap = (b) => {
  if (b.length >= 33 && b[0] === 0x01) lastShotAt = performance.now();
};

async function waitShot(since: number) {
  const t0 = performance.now();
  while (performance.now() - t0 < SHOT_WAIT_MS) {
    if (lastShotAt > since) return true;
    await pause(80);
  }
  return false;
}

/** Measure with the laser already on (it warmed up during the move); one retry. */
async function shoot() {
  for (let attempt = 0; attempt < 2; attempt++) {
    const since = performance.now();
    if (attempt) laser(true), await pause(400);
    takeShot();
    if (await waitShot(since)) return true;
  }
  return false;
}

// ---- guarded moves on the gimbal's report
class AtLimit extends Error {
  constructor(public at: Att) {
    super('at the edge of the safe range');
  }
}

async function fresh(): Promise<Att> {
  const a = await gimbal.freshAttitude(performance.now(), 3000);
  if (!a) throw new Error('The gimbal stopped reporting its position');
  return a;
}

const RATES = (axis: Axis, v: number): [number, number, number] => (axis === 'pan' ? [v, 0, 0] : axis === 'tilt' ? [0, v, 0] : [0, 0, v]);

/** Turns `axis` by `deg` without letting the roll joint (or the axis's own joint) leave its window: steps of at most
 * one second, each followed by the next report; before each step every joint is predicted from how much it moved per
 * degree on the step before (one axis moves several joints with the clamp on its side). Throws AtLimit at the edge. */
async function guardedTurn(axis: Axis, deg: number, dps: number): Promise<Att> {
  let now = await fresh();
  const dir = Math.sign(deg) || 1;
  const own = axis === 'pan' ? 0 : axis === 'tilt' ? 1 : 2;
  const per = [0, 0, 0];
  per[own] = axis === 'tilt' ? -1.3 : 1.3; // pitching up lowers the tilt joint
  let known = false;
  let done = 0;
  const w = [LIMITS.pan, LIMITS.tilt, LIMITS.roll];
  for (let i = 0; i < 64; i++) {
    const left = Math.abs(deg) - done;
    if (left < 1) return now;
    const j = jointsOf(now);
    let allowed = Math.min(left, dps);
    for (let k = 0; k < 3; k++) {
      if (k !== own && k !== 2) continue; // pan and tilt end at harmless stops; the roll limits every move
      const rate = Math.abs(per[k]) * (known ? 1.5 : 1);
      if (rate < 1e-3) continue;
      const edge = per[k] * dir > 0 ? w[k][1] - j[k] : j[k] - w[k][0];
      allowed = Math.min(allowed, Math.max(0, edge) / rate);
    }
    if (allowed < 2) throw new AtLimit(now);
    const [y, p, r] = RATES(axis, dir * dps);
    await gimbal.turn(y, p, r, (allowed / dps) * 1000);
    await pause(300);
    const next = await fresh();
    const turned = wrap180(along(next, axis) - along(now, axis)) * dir;
    done += turned;
    if (turned > 2) {
      const a = jointsOf(now), b = jointsOf(next);
      for (let k = 0; k < 3; k++) per[k] = ((b[k] - a[k]) / turned) * dir;
      known = true;
    } else if (allowed > 5) {
      throw new AtLimit(next); // asked to move and it did not: a hard stop
    }
    now = next;
  }
  return now;
}

/** guardedTurn, with stopping at the edge as an answer rather than an error. */
async function turnOrEdge(axis: Axis, deg: number, dps: number): Promise<Att> {
  try {
    return await guardedTurn(axis, deg, dps);
  } catch (e) {
    if (e instanceof AtLimit) return e.at;
    throw e;
  }
}

/** If the roll joint is outside its window (near a flip), turn it slowly back inside. */
async function backInside(): Promise<Att> {
  let now = await fresh();
  for (let i = 0; i < 6; i++) {
    const r = now.roll;
    const want = r < LIMITS.roll[0] + 2 ? LIMITS.roll[0] + 8 - r : r > LIMITS.roll[1] - 2 ? LIMITS.roll[1] - 8 - r : 0;
    if (!want) return now;
    await gimbal.turn(0, 0, Math.sign(want) * 8, (Math.abs(want) / 8) * 1000);
    await pause(300);
    now = await fresh();
  }
  return now;
}

/** Points the clamp top at `d`: the cheapest answer inside the windows; for a big pan it first stands the clamp
 * upright (nothing flips on the way), then pans, then rolls, then trims tilt, pan and roll on the report. `warm` is
 * called before the last trim (the laser warms up while the gimbal settles). Null when out of reach. */
async function moveToward(d: Direction, home: Att, warm: () => void = () => {}): Promise<Att | null> {
  let now = await backInside();
  const sol = solutions(d, home, now, LIMITS)[0];
  if (!sol) return null;
  const hr = euler(home.q)[0];
  if (Math.abs(sol.turnYaw) > 3) {
    const up = wrap180(hr - along(now, 'roll'));
    if (Math.abs(up) > 3) now = await turnOrEdge('roll', up, RATE);
    now = await turnOrEdge('pan', sol.turnYaw, RATE);
  }
  const roll = wrap180(sol.roll - along(now, 'roll'));
  if (Math.abs(roll) > 1) now = await turnOrEdge('roll', roll, RATE);
  warm();
  // trim; inclination follows roll as −1 on one side (r = −inc), +1 on the other (r = inc − 180)
  const incPerRoll = Math.cos((sol.roll * Math.PI) / 180) >= 0 ? -1 : 1;
  let best = Infinity;
  for (let i = 0; i < 6; i++) {
    const p = pointing(now, home);
    const vertical = d.inc > 85 || p.inc > 85;
    const daz = vertical ? 0 : wrap180(d.az - p.az), dinc = d.inc - p.inc;
    const err = Math.hypot(daz, dinc);
    // within reach, or trimming stopped helping (near vertical the tilt sets how high it gets: rolling on would only
    // lean it the other way, toward the flip)
    if (err < TOL || err > best - 0.3) break;
    best = err;
    const pitch = along(now, 'tilt');
    if (Math.abs(pitch) >= TOL) {
      now = await turnOrEdge('tilt', -pitch, TRIM_RATE);
      continue;
    }
    if (Math.abs(daz) >= TOL) now = await turnOrEdge('pan', daz, TRIM_RATE);
    if (Math.abs(dinc) >= TOL) now = await turnOrEdge('roll', dinc * incPerRoll, TRIM_RATE);
  }
  return now;
}

// ---- manual aiming: the stick moves the laser forward/back (tilt; push up = forward) and turns it (pan); two
// buttons lean it left/right (roll). The guard predicts each joint from the last report plus what has been sent
// since, and stops an axis a quarter second before its edge; a pan is held while it could drag the roll there.
/** Which tilt command sign takes the laser (out of the clamp's top) forward: camera nose down, as on phone gimbals. */
const TILT_FORWARD = -1;
let stick = { x: 0, y: 0 };
let rollWant = 0;
let aimTimer: ReturnType<typeof setInterval> | undefined;
let sent = { pan: 0, tilt: 0, roll: 0 };
let sentSince = { pan: 0, tilt: 0, roll: 0 };
let sentFrom = 0;
let lastTick = 0;
let busy = false;

function aimTick() {
  const a = gimbal.attitude;
  const now = performance.now();
  if (!a) {
    aim.atLimit = 'no report yet';
    gimbal.release();
    return;
  }
  // what was sent since the report, integrated from the report's time
  if (sentFrom < a.at) {
    sentFrom = a.at;
    sentSince = { pan: 0, tilt: 0, roll: 0 };
  }
  const dt = Math.min(0.2, (now - Math.max(sentFrom, lastTick)) / 1000);
  lastTick = now;
  sentSince.pan += sent.pan * dt;
  sentSince.tilt += sent.tilt * dt;
  sentSince.roll += sent.roll * dt;
  aim.atLimit = '';
  const max = AIM_SPEEDS[aim.speed];
  const shape = (v: number) => Math.sign(v) * v * v * max;
  const want = { pan: shape(stick.x), tilt: TILT_FORWARD * shape(stick.y), roll: rollWant };
  const look = 0.25;
  const j = jointsOf(a);
  // predicted joint (+ a look ahead at the wanted rate): each axis moves its own joint about 1 : 1 (tilt backwards)
  const guard = (k: 0 | 1 | 2, rate: number, scale: number, name: string) => {
    const moved = [sentSince.pan, -1.25 * sentSince.tilt, sentSince.roll][k];
    const pos = j[k] + moved * 1.2 + rate * scale * look;
    const [lo, hi] = [LIMITS.pan, LIMITS.tilt, LIMITS.roll][k];
    if ((rate * scale > 0 && pos > hi) || (rate * scale < 0 && pos < lo)) {
      aim.atLimit = name;
      return 0;
    }
    return rate;
  };
  let pan = guard(0, want.pan, 1, 'pan');
  const tilt = guard(1, want.tilt, -1.25, 'tilt');
  const roll = guard(2, want.roll, 1, 'roll');
  // a pan drags the roll joint by up to ~0.3 per degree: hold the pan while that could carry the roll to an edge
  const drag = 0.3 * Math.abs(sentSince.pan) + 0.3 * Math.abs(pan) * (look + 1);
  const rollPos = j[2] + sentSince.roll;
  if (pan && (rollPos - LIMITS.roll[0] < drag || LIMITS.roll[1] - rollPos < drag)) {
    pan = 0;
    aim.atLimit = 'roll (lean it back toward upright to pan)';
  }
  sent = { pan, tilt, roll };
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

/** Reset: back to home (pan centred, clamp upright, tilt level), on the gimbal's own report. */
export async function aimReset() {
  if (!gimbal.connected || dome.running || busy) return;
  const { home, why } = homeNow();
  if (!home) {
    dome.error = why;
    return;
  }
  busy = aim.resetting = true;
  aimRelease();
  dome.error = '';
  await pause(150);
  try {
    await moveToward({ az: 0, inc: 90 }, home);
  } catch (e) {
    dome.error = e instanceof Error ? e.message : String(e);
  } finally {
    busy = aim.resetting = false;
  }
}

// ---- scans: lists of directions, planned by the kinematics
let stopRequested = false;
export function stopDome() {
  stopRequested = true;
}
export function setScanKind(k: 'ring' | 'dome') {
  dome.kind = k;
  try {
    localStorage.setItem('cavecad.gimbal.scan', k);
  } catch {
    /* not kept: fine */
  }
}
/** Ring scan: the cross-section square to the passage (the gimbal set down facing the forward station), both walls
 * from straight up to 20° below level. */
export const startRing = () => startScan('ring');
/** Dome scan: the half-sphere above the horizon, every `step` of azimuth and inclination that the gimbal can reach. */
export const startDome = () => startScan('dome');

/** Where each shot really went, by the gimbal's report taken while the X1 measured (the plan is played without
 * correction, so this, not the plan, is the shot's direction). */
export const shotLog: { planned: Direction; got: Direction; joints: [number, number, number] }[] = [];

/** The roll joint must stay well clear of the flip (63 / 221): the one check made while a program plays. */
const FLIP_GUARD: [number, number] = [70, 212];

async function startScan(kind: 'ring' | 'dome') {
  if (dome.running || busy || !gimbal.connected || !survey.canTakeShot) return;
  const { home, why } = homeNow();
  if (!home) {
    dome.error = why;
    return;
  }
  const prog = program(kind, dome.level);
  aimRelease();
  dome.running = true;
  dome.error = '';
  dome.done = 0;
  dome.missed = prog.missed;
  dome.total = prog.shots;
  dome.taken = [];
  dome.current = '';
  shotLog.length = 0;
  stopRequested = false;
  if (!startDrivenSweep('lrud')) {
    dome.running = false;
    dome.error = 'Could not start a sweep here (is a station standing?)';
    return;
  }
  try {
    // set up: home, precisely, on the report (the program starts from there)
    dome.phase = 'Getting ready';
    await moveToward({ az: 0, inc: 90 }, home);
    // play the program as it stands
    for (const op of prog.ops) {
      if (stopRequested) break;
      const a = gimbal.attitude;
      if (a && (a.roll < FLIP_GUARD[0] || a.roll > FLIP_GUARD[1])) throw new Error(`Stopped: the roll got near the point where it flips (joint ${Math.round(a.roll)}°)`);
      const st = op.shot !== undefined ? prog.steps[op.shot] : null;
      if (st) {
        dome.current = dirKey(st.d);
        dome.phase = `${kind === 'dome' ? 'Dome' : 'Ring'} ${op.shot! + 1}/${prog.shots}`;
      }
      if (op.laser) laser(true);
      if (op.ms > 0) await gimbal.turn(op.yaw, op.pitch, op.roll, op.ms);
      if (!st) continue;
      await pause(SETTLE_S * 1000);
      const t = performance.now();
      if (await shoot()) {
        dome.done++;
        dome.taken = [...dome.taken, dirKey(st.d)];
        // the report that comes while the X1 measures says where it really went
        const r = await gimbal.freshAttitude(t, 1500);
        if (r) shotLog.push({ planned: st.d, got: pointing(r, home), joints: jointsOf(r) });
      }
    }
    dome.current = '';
    if (!stopRequested) {
      dome.phase = 'Home';
      await moveToward({ az: 0, inc: 90 }, home);
    }
  } catch (e) {
    dome.error = e instanceof Error ? e.message : String(e);
  } finally {
    await gimbal.stop().catch(() => {});
    laser(false);
    finishSweep();
    dome.running = false;
    dome.current = '';
    dome.phase = stopRequested ? 'Stopped' : `Done: ${dome.done} shots${dome.missed ? `, ${dome.missed} out of reach` : ''}`;
  }
}
