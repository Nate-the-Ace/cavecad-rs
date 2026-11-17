// Gimbal control for an X1 riding an Osmo Mobile 7, laser out of the clamp's top. CaveCAD asks for laser DIRECTIONS
// (azimuth from the gimbal's front, inclination). Scans and Reset run in TARGET MODE, DJI's own positioning (as Mimo
// runs a panorama): ./instrument/gimbal-ik turns directions into attitude targets (the X1's sag at home is in the maths),
// the whole scan is worked out in advance as a list of targets (big moves split into small, gentle steps), and here
// the list is sent in batches and stepped through, shooting on each "arrived". Nothing is worked out while it runs;
// the gimbal's report at each shot logs where it really went. Measured behaviour: tools/cave-osmo/FINDINGS.md.
//
// The joystick still drives with speed commands, guarded by the reported joints (speed commands CAN flip the roll).
// The gimbal is set down facing the forward station.
import { OsmoGimbal } from './instrument/osmo';
import {
  OM7_PORTRAIT,
  OM7_UPRIGHT_ROLL,
  plan,
  compileLists,
  programSeconds,
  ringDirections,
  domeDirections,
  homeFrom,
  pointing,
  jointsOf,
  euler,
  clampTop,
  wrap180,
  NOMINAL_SAG,
  type Att,
  type Direction,
  type ProgramStep,
} from './instrument/gimbal-ik';
import { survey, laser, takeShot, startDrivenSweep, finishSweep, keepInstrumentAwake, instrumentCommand } from './survey.svelte';
import { engine } from './engine';
import { cave } from './cave.svelte';

/** Said once in the trip's Instruments field after a gimbal sweep: the X1's compass is trusted, but read next to the
 * gimbal's motors, so the record says so. */
const GIMBAL_NOTE = 'gimbal sweeps: X1 compass read on a DJI gimbal (possible magnetic interference)';

function noteGimbalInNotebook() {
  try {
    if (!cave.trip) return;
    const page = engine.notebookPage(cave.trip) as { instruments?: string } | null;
    const now = (page?.instruments ?? '').trim();
    if (now.includes('magnetic interference')) return;
    const text = (now ? `${now}; ${GIMBAL_NOTE}` : `Cavway X1; ${GIMBAL_NOTE}`).slice(0, 200);
    const r = engine.notebookSetTripField(cave.trip, 'instruments', text);
    dlog(`notebook: instruments note -> ${r}`);
  } catch (e) {
    dlog(`notebook: could not add the gimbal note (${e instanceof Error ? e.message : e})`);
  }
}

/** After a move's own time, until the gimbal says "arrived" (s). */
const ARRIVE_S = 0.5;
/** Seconds to leave and re-enter target mode between lists (at home: no move). */
const LIST_S = 0.8;
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
/** The deepest a ring asks for below level (°): target mode reaches ~20° below on either wall. */
const RING_LOWEST = -20;
const LIMITS = OM7_PORTRAIT;
/** Smooth sweeps (the X1's own scan mode on CaveCAD firmware, -cc2 and later): the gimbal glides through the same
 * targets this many times slower than a stop-and-go scan moves (15°/s / 2), never stopping to shoot; the X1 keeps a
 * shot whenever it is steady and pointing somewhere new. */
const SMOOTH_SLOW = 2;
/** Scan control bytes (tools/x1-patch). */
const SCAN_START = 0x39, SCAN_STOP = 0x3a, SCAN_STATUS = 0x3b;

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
  /** The clamp's sag at home (°), measured at the start of each scan; planning uses it. */
  sag: NOMINAL_SAG,
  /** Smooth sweep (X1 scan mode while the gimbal glides) when the X1 runs CaveCAD firmware; else stop-and-go. */
  smooth: ((): boolean => {
    try {
      return localStorage.getItem('cavecad.gimbal.smooth') !== 'off';
    } catch {
      return true;
    }
  })(),
  /** The X1 answered CaveCAD firmware's scan status (null: not asked yet). */
  ccFirmware: null as boolean | null,
  running: false,
  phase: '',
  done: 0,
  total: 0,
  missed: 0,
  error: '',
});
gimbal.onChange = () => {
  // the X1 rides the gimbal unattended: keep it from sleeping while the gimbal is connected
  if (gimbal.connected !== dome.connected) keepInstrumentAwake(gimbal.connected);
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
/** Into the app's debug log (pulled off the phone to see what a scan did). */
const dlog = (m: string) => (globalThis as { __cavecadDlog?: (s: string) => void }).__cavecadDlog?.(m);

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

/** A direction's key, for marking progress. */
export const dirKey = (d: Direction) => `${Math.round(d.az * 10)},${Math.round(d.inc * 10)}`;

type Program = { lists: ProgramStep[][]; steps: ReturnType<typeof plan>['steps']; points: (Direction & { ok: boolean; key: string })[]; shots: number; missed: number; secs: number };
// worked out once per scan, level and sag (to the degree), never while it runs
const programs = new Map<string, Program>();
function program(kind: 'ring' | 'dome', level: number, sag = dome.sag): Program {
  const key = `${kind}:${level}:${Math.round(sag)}`;
  let got = programs.get(key);
  if (!got) {
    const step = LEVELS[level].step;
    const dirs = kind === 'dome' ? domeDirections(step) : ringDirections(0, step, RING_LOWEST);
    const pl = plan(dirs, Math.round(sag));
    const lists = compileLists(pl);
    const miss = new Set(pl.unreachable);
    got = {
      lists,
      steps: pl.steps,
      points: dirs.map((d) => ({ ...d, ok: !miss.has(d), key: dirKey(d) })),
      shots: pl.steps.length,
      missed: pl.unreachable.length,
      secs: programSeconds(lists.flat(), { arriveS: ARRIVE_S, shotS: SHOT_S, settleS: SETTLE_S }) + lists.length * LIST_S,
    };
    if (programs.size > 40) programs.clear();
    programs.set(key, got);
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
/** Scan shots seen (bit 6 of the flag clear), and the last scan-control answer from CaveCAD firmware. */
let scanShots = 0;
type ScanState = { cmd: number; mode: number; laser: number; count: number; result: number; lastEnd: number; at: number };
let lastScan: ScanState | null = null;
(globalThis as { __cavecadShotTap?: (b: Uint8Array) => void }).__cavecadShotTap = (b) => {
  if (b.length >= 33 && b[0] === 0x01) {
    lastShotAt = performance.now();
    if ((b[1] & 0x40) === 0) {
      scanShots++;
      if (dome.running && dome.smooth) dome.done = scanShots;
    }
  }
  // [3D, cmd, CC, 12, mode, laser, count u16, ticks u32, options, result, last end, version]
  if (b.length >= 16 && b[0] === 0x3d && b[2] === 0xcc && b[3] === 12 && b[1] >= SCAN_START && b[1] <= SCAN_STATUS) {
    lastScan = { cmd: b[1], mode: b[4], laser: b[5], count: b[6] | (b[7] << 8), result: b[13], lastEnd: b[14], at: performance.now() };
  }
};

/** Sends a scan-control byte and waits for its answer (null: none; the X1 is not on CaveCAD firmware). */
async function scanAsk(cmd: number, ms = 1500): Promise<ScanState | null> {
  const since = performance.now();
  instrumentCommand(cmd);
  while (performance.now() - since < ms) {
    if (lastScan && lastScan.at > since && lastScan.cmd === cmd) return lastScan;
    await pause(60);
  }
  return null;
}

/** Starts the X1's scan mode (laser on first); a few tries while the laser warms. */
async function scanStart(): Promise<boolean> {
  for (let i = 0; i < 6; i++) {
    const r = await scanAsk(SCAN_START);
    dlog(`smooth: scan start -> ${JSON.stringify(r)}`);
    if (r?.result === 0) return true;
    if (r?.result === 1 && r.mode !== 2) laser(true); // wrong mode: the laser is off
    await pause(r?.result === 1 ? 1600 : 500);
  }
  return false;
}

export function setSmooth(on: boolean) {
  dome.smooth = on;
  try {
    localStorage.setItem('cavecad.gimbal.smooth', on ? 'on' : 'off');
  } catch {
    /* not kept: fine */
  }
}

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

// ---- target mode runs
async function fresh(): Promise<Att> {
  const a = await gimbal.freshAttitude(performance.now(), 3000);
  if (!a) throw new Error('The gimbal stopped reporting its position');
  return a;
}

let listId = 0x30;
/** Into target mode and home (target 0/0); returns the sag and heading measured there. */
async function targetsHome(): Promise<{ sag: number; heading: number }> {
  await gimbal.targetsEnter();
  await pause(1500);
  const id = (listId = ((listId + 1) & 0x7f) | 0x30);
  await gimbal.targetsUpload([{ yaw: 0, roll: 0 }], id);
  const r = await gimbal.targetsArrived(0, id, 6000);
  dlog(`gimbal home: ${r} (status ${JSON.stringify(gimbal.targets)})`);
  if (r !== 'arrived') throw new Error(r === 'aborted' ? 'The gimbal would not take its targets (let it settle upright and try again)' : 'The gimbal did not answer');
  await pause(400);
  const a = await fresh();
  const [, pitch, yaw] = euler(a.q);
  return { sag: pitch, heading: yaw };
}

/** Plays precalculated lists, each home to home: a list is sent (the gimbal sets off for its first target), each next
 * target stepped to and waited for, `onArrive` called at each (the shot). Between lists target mode is left and entered
 * again (the gimbal can only take a new list that way; entering sends it home, where the last list just ended). A list
 * the gimbal drops is run once more from the start (dropping sends it home, where every list starts). */
async function playTargets(lists: ProgramStep[][], onArrive: (st: ProgramStep) => Promise<void>) {
  for (const [li, list] of lists.entries()) {
    for (let attempt = 0; ; attempt++) {
      if (stopRequested) return;
      // every list (the first too: getting ready left the home list loaded) goes in fresh: leave, enter again (at
      // home: no move), give the gimbal a moment to settle into target mode, send
      if (!(await gimbal.targetsLeave())) dlog('gimbal: no "cleared" after leaving target mode; carrying on');
      await pause(300);
      await gimbal.targetsEnter(true);
      await pause(2500);
      const id = (listId = ((listId + 1) & 0x7f) | 0x30);
      if (list[0].shot !== undefined) laser(true);
      await gimbal.targetsUpload(list.map((st) => ({ ...st.t, ms: st.ms })), id);
      let k = 0;
      for (; k < list.length; k++) {
        if (stopRequested) return;
        if (k > 0) {
          if (list[k].shot !== undefined) laser(true);
          await gimbal.targetsGo(k);
        }
        let r = await gimbal.targetsArrived(k, id, list[k].ms + 5000);
        if (r === 'timeout') {
          // seen once on the phone: the first target of a fresh list sat "moving"; asking again got it there
          await gimbal.targetsGo(k);
          r = await gimbal.targetsArrived(k, id, list[k].ms + 5000);
        }
        dlog(`gimbal list ${li + 1}/${lists.length} target ${k + 1}/${list.length} yaw ${list[k].t.yaw.toFixed(1)} roll ${list[k].t.roll.toFixed(1)}: ${r} (status ${JSON.stringify(gimbal.targets)})`);
        if (r !== 'arrived') break;
        // target mode ends by itself after ~9 s without a command: while the shot is taken, ask for this target again
        // every 3 s (it is already there: no move)
        const hold = setInterval(() => void gimbal.targetsGo(k), 3000);
        try {
          await onArrive(list[k]);
        } finally {
          clearInterval(hold);
        }
      }
      if (k === list.length) break;
      if (attempt >= 1) throw new Error('The gimbal keeps dropping its targets (too much weight? let it settle and try again)');
    }
  }
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

/** Reset: home (target 0/0: the gimbal's own front, upright as it sits), in target mode, then out. */
export async function aimReset() {
  if (!gimbal.connected || dome.running || busy) return;
  busy = aim.resetting = true;
  aimRelease();
  dome.error = '';
  await pause(150);
  try {
    const h = await targetsHome();
    dome.sag = h.sag;
    await gimbal.targetsExit();
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

/** Where each shot really went, by the gimbal's report taken while the X1 measured (the planned target puts it within
 * ~1.5° of the plan; this is the record). Azimuth from the gimbal's front at home. */
export const shotLog: { planned: Direction; got: Direction; joints: [number, number, number] }[] = [];

/** The smooth sweep: laser on, the X1's scan started, the gimbal plays the targets slowly without stopping, the scan is
 * restarted if the X1 ends it on its own (its time or 200-shot limit), and stopped at the end. */
async function smoothScan(kind: 'ring' | 'dome', prog: Program) {
  scanShots = 0;
  dome.done = 0;
  dome.phase = 'Starting the X1 scan';
  laser(true);
  await pause(1700);
  if (!(await scanStart())) throw new Error("The X1 wouldn't start its scan (is the laser on? try again)");
  let restarting = false;
  const watch = setInterval(async () => {
    if (restarting || stopRequested) return;
    const r = await scanAsk(SCAN_STATUS, 1200);
    if (r && r.mode !== 2 && !stopRequested) {
      restarting = true;
      dlog(`smooth: scan ended on the X1 (last end ${r.lastEnd}); starting it again`);
      await scanStart();
      restarting = false;
    }
  }, 2500);
  try {
    const slow = prog.lists.map((l) => l.map((st) => ({ ...st, ms: Math.round(st.ms * SMOOTH_SLOW) })));
    dome.phase = `Smooth ${kind}`;
    await playTargets(slow, async (st) => {
      if (st.shot !== undefined) dome.current = dirKey(prog.steps[st.shot].d);
    });
  } finally {
    clearInterval(watch);
    await scanAsk(SCAN_STOP, 1200);
    dlog(`smooth: done, ${scanShots} scan shots`);
  }
}

async function startScan(kind: 'ring' | 'dome') {
  if (dome.running || busy || !gimbal.connected || !survey.canTakeShot) return;
  aimRelease();
  dome.running = true;
  dome.error = '';
  dome.done = 0;
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
    dome.phase = 'Getting ready';
    const home = await targetsHome();
    dome.sag = home.sag;
    // the program for this sag: worked out before (cached) unless the sag is new
    const prog = program(kind, dome.level, home.sag);
    let missedInARow = 0;
    dome.missed = prog.missed;
    dome.total = prog.shots;
    // smooth: the X1 scans on its own while the gimbal glides (CaveCAD firmware); else stop-and-go below
    if (dome.smooth) {
      const st = await scanAsk(SCAN_STATUS);
      dome.ccFirmware = st !== null;
      if (st) {
        await smoothScan(kind, prog);
        return;
      }
      dlog('smooth: no scan-control answer (stock firmware?); stop-and-go instead');
      dome.phase = 'Stop-and-go (the X1 has no remote scan control)';
    }
    await playTargets(prog.lists, async (st) => {
      if (st.shot === undefined) return;
      const planned = prog.steps[st.shot].d;
      dome.current = dirKey(planned);
      dome.phase = `${kind === 'dome' ? 'Dome' : 'Ring'} ${st.shot + 1}/${prog.shots}`;
      await pause(SETTLE_S * 1000);
      const t = performance.now();
      if (!(await shoot())) {
        missedInARow++;
        dlog(`gimbal scan: no shot came back (${missedInARow} in a row)`);
        if (missedInARow >= 2) throw new Error("The X1 isn't answering (no shot came back twice): wake it, check it is connected, then start again");
      } else {
        missedInARow = 0;
        dome.done++;
        dome.taken = [...dome.taken, dirKey(planned)];
        const r = await gimbal.freshAttitude(t, 1500);
        if (r) {
          const top = clampTop(r.q);
          shotLog.push({ planned, got: { az: wrap180(top.az - home.heading), inc: top.inc }, joints: jointsOf(r) });
        }
      }
    });
    dome.current = '';
  } catch (e) {
    dome.error = e instanceof Error ? e.message : String(e);
    dlog(`gimbal scan stopped: ${dome.error}`);
  } finally {
    await gimbal.targetsExit().catch(() => {});
    laser(false);
    finishSweep();
    if (dome.done > 0) noteGimbalInNotebook();
    dome.running = false;
    dome.current = '';
    dome.phase = stopRequested ? 'Stopped' : `Done: ${dome.done} shots${dome.missed ? `, ${dome.missed} out of reach` : ''}`;
    dome.current = '';
  }
}
