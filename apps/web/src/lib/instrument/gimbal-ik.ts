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
    const rollJoint = home.roll + wrap180(roll - hr);
    if (rollJoint < limits.roll[0] || rollJoint > limits.roll[1]) continue;
    // straight up has no azimuth: face home, the only place the tilt can stand the clamp fully upright
    const targetYaw = inc > 89.5 ? hy : hy + yawRel;
    const base = wrap180(targetYaw - ny);
    for (const turnYaw of [base, base - 360, base + 360]) {
      const panJoint = nj[0] + turnYaw;
      if (panJoint < limits.pan[0] || panJoint > limits.pan[1]) continue;
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

/** Orders the reachable targets so the gimbal moves little (always the nearest next, straight up first) and says
 * which are out of reach. Each step carries the solution as planned from the one before. */
export function plan(dirs: Direction[], home: Att, from: Att = home, limits: JointLimits = OM7_PORTRAIT) {
  const unreachable: Direction[] = [];
  let left = dirs.filter((d) => {
    const ok = solutions(d, home, home, limits).length > 0;
    if (!ok) unreachable.push(d);
    return ok;
  });
  const steps: { d: Direction; s: Solution }[] = [];
  let at = from;
  let travel = 0;
  const up = left.findIndex((d) => d.inc >= 89.5);
  if (up >= 0) left = [left[up], ...left.slice(0, up), ...left.slice(up + 1)];
  let first = up >= 0;
  while (left.length) {
    let best = -1, bestCost = Infinity, bestSol: Solution | null = null;
    for (const [i, d] of left.entries()) {
      if (first && i > 0) break;
      const s = solutions(d, home, at, limits)[0];
      if (!s) continue;
      const c = Math.abs(s.turnYaw) + Math.abs(wrap180(s.roll - euler(at.q)[0]));
      if (c < bestCost) [best, bestCost, bestSol] = [i, c, s];
    }
    first = false;
    if (best < 0 || !bestSol) {
      unreachable.push(...left);
      break;
    }
    steps.push({ d: left[best], s: bestSol });
    travel += bestCost;
    at = landing(bestSol, at);
    left.splice(best, 1);
  }
  return { steps, unreachable, travel };
}
