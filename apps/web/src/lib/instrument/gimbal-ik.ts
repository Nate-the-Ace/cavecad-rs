// Gimbal kinematics for an X1 riding an Osmo Mobile 7 with its laser out of the clamp's top, from the gimbal's own
// attitude report (tools/cave-osmo/FINDINGS.md; the same model as the Rust library's `aim` module, bench-tested there).
//
// The gimbal reports, once a second, its attitude as a quaternion (north-east-down, north = wherever it woke up) and
// its joint angles. Speed commands turn the attitude's Euler angles: pan = yaw, tilt = pitch, roll = roll. With pitch
// held near 0 the clamp top points at inclination asin(−sin r), azimuth y + 90° when cos r > 0 and y − 90° when
// cos r < 0 (upright in portrait is r = −90°). So each direction has two answers: r = −inc with y = az − 90°, or
// r = inc − 180° with y = az + 90°. An answer counts only if the joints stay inside the safe windows: the roll has no
// stop, it FLIPS OVER (violently, at any speed) past joint ~63 and ~221, so most targets are reached by panning to face
// them from the long roll side ("going round the other way") instead of crossing the flip.
// Everything is in degrees.

/** w x y z */
export type Quat = [number, number, number, number];

/** One attitude report: the quaternion and the joints as the gimbal gives them. */
export interface Att {
  q: Quat;
  pan: number;
  tilt: number;
  roll: number;
}

/** A laser direction relative to the gimbal's front at home: azimuth clockwise from the front, inclination up. */
export interface Direction {
  az: number;
  inc: number;
}

export interface JointLimits {
  pan: [number, number];
  tilt: [number, number];
  roll: [number, number];
}

/** Safe joint windows in portrait (range test 2026-11-17, less a margin). Pan and tilt end at harmless stops; the
 * roll window is the one that matters (measured clean 63…221). */
export const OM7_PORTRAIT: JointLimits = { pan: [-198, 84], tilt: [-11, 31], roll: [75, 205] };
/** The roll joint at rest upright in portrait was 93–97 every time; outside this the clamp went portrait some other
 * way and the windows were not measured for it. */
export const OM7_UPRIGHT_ROLL: [number, number] = [80, 112];

const DEG = 180 / Math.PI;

/** How far panning drags the roll joint with the clamp upright, by pan joint (fit to the range test 2026-11-17: pan
 * +86 → −23, +42 → −13, −33 → +8, −94 → +12, −159 → −3, −189 → −13; within ~3°). */
export const panRollDrag = (pan: number) => -16 * Math.sin(pan / DEG) - 4 * (1 - Math.cos(pan / DEG));

export const wrap180 = (a: number) => {
  let x = ((a % 360) + 360) % 360;
  if (x > 180) x -= 360;
  return x;
};

/** The pan joint unwrapped onto the OM7's stops (−202 … +92; readings wrap at ±180). */
export const om7Pan = (j: number) => (j > 97 ? j - 360 : j);
export const jointsOf = (a: Att): [number, number, number] => [om7Pan(a.pan), a.tilt, a.roll];

/** Euler angles (roll, pitch, yaw). */
export function euler(q: Quat): [number, number, number] {
  const [w, x, y, z] = q;
  return [
    Math.atan2(2 * (w * x + y * z), 1 - 2 * (x * x + y * y)) * DEG,
    Math.asin(Math.max(-1, Math.min(1, 2 * (w * y - z * x)))) * DEG,
    Math.atan2(2 * (w * z + x * y), 1 - 2 * (y * y + z * z)) * DEG,
  ];
}

export function fromEuler(roll: number, pitch: number, yaw: number): Quat {
  const [r, p, y] = [roll / DEG / 2, pitch / DEG / 2, yaw / DEG / 2];
  const [cr, sr, cp, sp, cy, sy] = [Math.cos(r), Math.sin(r), Math.cos(p), Math.sin(p), Math.cos(y), Math.sin(y)];
  return [cr * cp * cy + sr * sp * sy, sr * cp * cy - cr * sp * sy, cr * sp * cy + sr * cp * sy, cr * cp * sy - sr * sp * cy];
}

/** The rotation between two attitudes. */
export function angleBetween(a: Quat, b: Quat) {
  const d = Math.abs(a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3]);
  return 2 * Math.acos(Math.min(1, d)) * DEG;
}

export type Axis = 'pan' | 'tilt' | 'roll';
/** The Euler angle an axis's speed command turns. */
export const along = (a: Att, axis: Axis) => {
  const [r, p, y] = euler(a.q);
  return axis === 'pan' ? y : axis === 'tilt' ? p : r;
};

/** Where the clamp top (body +Y) points in the gimbal's world: azimuth from its north, inclination. */
export function clampTop(q: Quat): Direction {
  const [w, x, y, z] = q;
  // body (0,1,0) rotated: the quaternion's second column
  const n = 2 * (x * y - w * z), e = 1 - 2 * (x * x + z * z), d = 2 * (y * z + w * x);
  return { az: Math.atan2(e, n) * DEG, inc: Math.asin(Math.max(-1, Math.min(1, -d))) * DEG };
}

/** Where the clamp top points relative to the gimbal's front at `home`. */
export function pointing(a: Att, home: Att): Direction {
  const t = clampTop(a.q);
  return { az: wrap180(t.az - euler(home.q)[2]), inc: t.inc };
}

/** A home from wherever the gimbal is: same heading the pan centre faces, clamp upright, joints as they would be. */
export function homeFrom(a: Att): Att {
  const [r, p, y] = euler(a.q);
  const pan = om7Pan(a.pan);
  return { q: fromEuler(-90, p, y - pan), pan: 0, tilt: a.tilt, roll: a.roll - wrap180(r + 90) };
}

/** One way to point there: the yaw to turn (signed, the way round that stays between the pan stops), the roll Euler
 * angle to end at, and the joints it should land on. */
export interface Solution {
  turnYaw: number;
  roll: number;
  panJoint: number;
  rollJoint: number;
}

/** Every answer for `d` from `now`, measured against `home`, inside `limits`; cheapest first. */
export function solutions(d: Direction, home: Att, now: Att, limits: JointLimits = OM7_PORTRAIT): Solution[] {
  const [hr, , hy] = euler(home.q);
  const [nr, , ny] = euler(now.q);
  const nj = jointsOf(now);
  const inc = Math.max(-90, Math.min(90, d.inc));
  const out: Solution[] = [];
  for (const [r0, yawRel] of [
    [-inc, d.az - 90],
    [inc - 180, d.az + 90],
  ]) {
    const roll = wrap180(r0);
    const rollTurn = wrap180(roll - hr);
    // straight up has no azimuth: face home, the only place the tilt can stand the clamp fully upright
    const targetYaw = inc > 89.5 ? hy : hy + yawRel;
    const base = wrap180(targetYaw - ny);
    for (const turnYaw of [base, base - 360, base + 360]) {
      const panJoint = nj[0] + turnYaw;
      if (panJoint < limits.pan[0] || panJoint > limits.pan[1]) continue;
      // the roll joint there: home's, plus the roll turned, plus what the pan drags it by
      const rollJoint = home.roll + rollTurn + panRollDrag(panJoint) - panRollDrag(home.pan);
      if (rollJoint < limits.roll[0] || rollJoint > limits.roll[1]) continue;
      out.push({ turnYaw, roll, panJoint, rollJoint });
    }
  }
  const cost = (s: Solution) => Math.abs(s.turnYaw) + Math.abs(wrap180(s.roll - nr));
  return out.sort((a, b) => cost(a) - cost(b));
}

/** The attitude a solution should land on (pitch held), for planning the next move from it. */
export function landing(s: Solution, now: Att): Att {
  const [, p, y] = euler(now.q);
  return { q: fromEuler(s.roll, p, y + s.turnYaw), pan: s.panJoint, tilt: now.tilt, roll: s.rollJoint };
}

/** Directions for a ring across the passage: the plane square to `passageAz` (where the passage goes, from the
 * gimbal's front), from straight up down both walls to `lowest`, every `step`. */
export function ringDirections(passageAz: number, step: number, lowest = -90): Direction[] {
  const out: Direction[] = [{ az: wrap180(passageAz + 90), inc: 90 }];
  for (const wall of [90, -90]) {
    for (let inc = 90 - step; inc >= lowest - 1e-9; inc -= step) out.push({ az: wrap180(passageAz + wall), inc });
  }
  return out;
}

/** Directions for the dome: straight up, then every `step` of azimuth all the way round, every `step` of
 * inclination from near the top down to level. */
export function domeDirections(step: number): Direction[] {
  const out: Direction[] = [{ az: 0, inc: 90 }];
  for (let az = -180; az < 180 - 1e-9; az += step) for (let inc = 90 - step; inc >= -1e-9; inc -= step) out.push({ az, inc });
  return out;
}

// ---- target mode (DJI's waypoints; tools/cave-osmo FINDINGS "DJI's positioning protocol") -------------------------
// The gimbal is given attitude targets (yaw, roll; pitch left at its home sag) and solves its motors itself: no flip,
// ~1 s a move whatever its size, "arrived" reported. Measured: the clamp ends at roll −90 − r about a yaw axis that is
// itself tilted back by the home sag (with the X1 aboard the handle/clamp sag ~16° back at home; left in, accounted for
// here, not commanded away). So the clamp top's direction for a target, relative to home's heading (north-east-down):
//   v = Ry(sag) · Rz(yaw) · (0, cos ra, sin ra), ra = −90 − roll;  inc = asin(−v_down), az = atan2(v_east, v_north)
// Fits every bench point to ~1.5° (target 0/45 → −105.3 / 42.8 measured −105.6 / 42.9; −180/45 → 105.3 / 42.8
// measured 104.7 / 44.5; −90/90 → 180 / −15.8 measured 178.9 / −14.4).

/** A target as the gimbal takes it: degrees, relative to home. */
export interface Target {
  yaw: number;
  roll: number;
}

/** What target mode allows with the X1 aboard: roll −35…+110 (empty, the gimbal goes to ~−77 on the − side, but that
 * is the short side, where the X1 hangs out sideways: on the phone a move to −61 overloaded it and it went home;
 * + tested to 110 = 20° below level);
 * yaw −200…+95 (lopsided like the pan stops: −195 and −184 run, +120 is taken clamped, +176…180 is refused with 0x90;
 * a yaw past +95 is asked for 360° lower). Each target carries its move time (the gimbal's per-target field is the
 * move's duration in hundredths of a second: 300 → a 45° move in ~3.5 s, 1000 → 10.4 s; Mimo sends 0.6 s, too quick
 * with the X1): moves run at `dps`, never under `minMs`; moves longer than `maxStep` are split so the gimbal's path
 * stays close to the planned one. */
export const OM7_TARGETS = { roll: [-35, 110] as [number, number], yaw: [-200, 95] as [number, number], maxStep: 45, dps: 15, minMs: 600 };

/** The home sag measured on the bench with the X1 aboard (°): used to plan before the gimbal reports its own. */
export const NOMINAL_SAG = 16;

/** Where the clamp top points for a target, relative to home's heading, with the clamp sagging `sag` at home. */
export function targetPointing(t: Target, sag: number): Direction {
  const ra = (-90 - t.roll) / DEG, p = sag / DEG, y = t.yaw / DEG;
  // w = Rz(yaw) · (0, cos ra, sin ra)
  const wx = -Math.cos(ra) * Math.sin(y), wy = Math.cos(ra) * Math.cos(y), wz = Math.sin(ra);
  // v = Ry(sag) · w
  const n = wx * Math.cos(p) + wz * Math.sin(p), e = wy, d = -wx * Math.sin(p) + wz * Math.cos(p);
  return { az: Math.atan2(e, n) * DEG, inc: Math.asin(Math.max(-1, Math.min(1, -d))) * DEG };
}

/** Every target that points at `d` within the reach, cheapest from `from` first. */
export function targetSolutions(d: Direction, sag: number, from: Target = { yaw: 0, roll: 0 }, reach = OM7_TARGETS): Target[] {
  const p = sag / DEG, az = d.az / DEG, inc = d.inc / DEG;
  const n = Math.cos(inc) * Math.cos(az), e = Math.cos(inc) * Math.sin(az), dn = -Math.sin(inc);
  // w = Ry(−sag) · v
  const wx = n * Math.cos(p) - dn * Math.sin(p), wy = e, wz = n * Math.sin(p) + dn * Math.cos(p);
  const out: Target[] = [];
  const a = Math.asin(Math.max(-1, Math.min(1, wz)));
  for (const ra of [a, Math.PI - a]) {
    const c = Math.cos(ra);
    if (Math.abs(c) < 1e-6) {
      // the clamp top along the (tilted) yaw axis: any yaw does; keep it still
      out.push({ yaw: from.yaw, roll: wrap180(-90 - ra * DEG) });
      continue;
    }
    const yaw0 = Math.atan2(-wx / c, wy / c) * DEG;
    const roll = wrap180(-90 - ra * DEG);
    if (roll < reach.roll[0] || roll > reach.roll[1]) continue;
    // the same heading a turn either way, wherever it fits the lopsided yaw range
    for (const yaw of [yaw0, yaw0 - 360, yaw0 + 360]) if (yaw >= reach.yaw[0] && yaw <= reach.yaw[1]) out.push({ yaw, roll });
  }
  const cost = (t: Target) => Math.abs(t.yaw - from.yaw) + Math.abs(t.roll - from.roll);
  return out.sort((x, y) => cost(x) - cost(y));
}

/** One planned shot: the direction asked for, the target that takes it there, and its place in the order. */
export interface PlannedShot {
  d: Direction;
  t: Target;
  col: number;
}

/** Plans a scan: every reachable direction as a target, in the order that moves least: from home, always the nearest
 * remaining shot (by yaw plus roll to turn), each shot taking whichever of its two targets is nearer from there. */
export function plan(dirs: Direction[], sag: number, reach = OM7_TARGETS) {
  const unreachable: Direction[] = [];
  const left: { d: Direction; ts: Target[] }[] = [];
  for (const d of dirs) {
    const ts = targetSolutions(d, sag, { yaw: 0, roll: 0 }, reach);
    if (ts.length) left.push({ d, ts });
    else unreachable.push(d);
  }
  const steps: PlannedShot[] = [];
  let at: Target = { yaw: 0, roll: 0 };
  const cost = (t: Target) => Math.abs(t.yaw - at.yaw) + Math.abs(t.roll - at.roll);
  while (left.length) {
    let bi = 0, bt = left[0].ts[0], bc = Infinity;
    for (const [i, c] of left.entries())
      for (const t of c.ts) {
        const k = cost(t);
        if (k < bc) [bi, bt, bc] = [i, t, k];
      }
    steps.push({ d: left[bi].d, t: bt, col: steps.length });
    at = bt;
    left[bi] = left[left.length - 1];
    left.pop();
  }
  return { steps, unreachable, columns: steps.length };
}

/** One target of a precalculated program; `shot` = which planned shot to take on arrival (none: a travel step). */
export interface ProgramStep {
  t: Target;
  /** How long the move to this target takes (the gimbal's own pacing). */
  ms: number;
  shot?: number;
}

/** Targets in one list the gimbal will run (16 or more are taken but never stepped through). */
export const LIST_MAX = 15;
/** Most steps on the way out from home or back to it, in one list (the pace is set by each move's time, so a leg needs
 * few steps; they only keep the path near the planned one). */
const LEG_STEPS = 3;

const HOME: Target = { yaw: 0, roll: 0 };
const span = (a: Target, b: Target) => Math.max(Math.abs(b.yaw - a.yaw), Math.abs(b.roll - a.roll));
/** The steps from `a` to `b` (b included), each at most `step`. */
function legTo(a: Target, b: Target, step: number): Target[] {
  const n = Math.max(1, Math.ceil(span(a, b) / step - 1e-9));
  return Array.from({ length: n }, (_, k) => ({ yaw: a.yaw + ((b.yaw - a.yaw) * (k + 1)) / n, roll: a.roll + ((b.roll - a.roll) * (k + 1)) / n }));
}
const legSteps = (a: Target, b: Target, step: number, cap: number) => Math.min(cap, Math.max(1, Math.ceil(span(a, b) / step - 1e-9)));

/** The whole scan, worked out in advance, as lists the gimbal runs one after another. Every list starts at home and
 * ends at home (a new list always starts from home: the gimbal goes there itself on entering target mode, so ending
 * there makes that a non-move): out from home in at most LEG_STEPS gentle steps, a patch of neighbouring shots in plan
 * order (steps of at most `maxStep` between them), back home in at most LEG_STEPS; never more than LIST_MAX targets. */
export function compileLists(p: ReturnType<typeof plan>, reach = OM7_TARGETS): ProgramStep[][] {
  const lists: ProgramStep[][] = [];
  const legStep = (a: Target, b: Target) => Math.max(reach.maxStep, span(a, b) / LEG_STEPS);
  const ms = (a: Target, b: Target) => Math.max(reach.minMs, (span(a, b) / reach.dps) * 1000);
  // each step's time from the target before it in the list (home before the first)
  const timed = (l: { t: Target; shot?: number }[]): ProgramStep[] => {
    let at = HOME;
    return l.map((st) => {
      const out = { ...st, ms: ms(at, st.t) };
      at = st.t;
      return out;
    });
  };
  let i = 0;
  while (i < p.steps.length) {
    const first = p.steps[i].t;
    const list: { t: Target; shot?: number }[] = legTo(HOME, first, legStep(HOME, first)).map((t) => ({ t }));
    list[list.length - 1].shot = i;
    let at = first;
    i++;
    // take the next shots while the list still has room for them and the way home
    while (i < p.steps.length) {
      const nx = p.steps[i].t;
      const between = legTo(at, nx, reach.maxStep);
      const home = legSteps(nx, HOME, legStep(nx, HOME), LEG_STEPS);
      if (list.length + between.length + home > LIST_MAX) break;
      list.push(...between.map((t) => ({ t })));
      list[list.length - 1].shot = i;
      at = nx;
      i++;
    }
    list.push(...legTo(at, HOME, legStep(at, HOME)).map((t) => ({ t })));
    lists.push(timed(list));
  }
  return lists;
}

/** All the lists end to end (the order the gimbal runs them). */
export const compile = (p: ReturnType<typeof plan>, reach = OM7_TARGETS): ProgramStep[] => compileLists(p, reach).flat();

/** How long a program takes: each move's own time, the gimbal's "arrived" (~0.5 s after), and a settle and the shot
 * (~1.5 s, the laser warmed during the move) for each shot. */
export function programSeconds(prog: ProgramStep[], o: { arriveS: number; shotS: number; settleS: number }) {
  return prog.reduce((t, st) => t + st.ms / 1000 + o.arriveS + (st.shot !== undefined ? o.shotS + o.settleS : 0), 0);
}
