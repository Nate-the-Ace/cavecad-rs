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

/** One planned shot: the direction and its answer measured from home (turnYaw = yaw from home). */
export interface PlannedShot {
  d: Direction;
  s: Solution;
  /** Which column (one yaw, the shots in it differ only in roll). */
  col: number;
}

/** Plans a scan for smooth, fast running: every reachable direction solved from home, grouped into columns of one yaw
 * (inside a column only the roll moves, in small steps, in roll order so it never doubles back), columns swept from
 * one pan end to the other with the roll zig-zagging. Says which directions are out of reach. */
export function plan(dirs: Direction[], home: Att, limits: JointLimits = OM7_PORTRAIT) {
  const unreachable: Direction[] = [];
  const cols = new Map<number, { d: Direction; s: Solution }[]>();
  for (const d of dirs) {
    const s = solutions(d, home, home, limits)[0];
    if (!s) {
      unreachable.push(d);
      continue;
    }
    const k = Math.round(s.turnYaw * 10) / 10;
    if (!cols.has(k)) cols.set(k, []);
    cols.get(k)!.push({ d, s });
  }
  const steps: PlannedShot[] = [];
  // start at the column nearest home (no pan), then sweep to the nearer pan end, then the far side
  const keys = [...cols.keys()].sort((a, b) => a - b);
  const zero = keys.reduce((b, k) => (Math.abs(k) < Math.abs(b) ? k : b), keys[0] ?? 0);
  const lower = keys.filter((k) => k < zero).reverse(), upper = keys.filter((k) => k > zero);
  const order = [zero, ...(upper.length <= lower.length ? [...upper, ...lower] : [...lower, ...upper])].filter((k) => cols.has(k));
  const hr = euler(home.q)[0];
  let rollAt = hr;
  for (const [i, k] of order.entries()) {
    const col = cols.get(k)!.sort((a, b) => a.s.roll - b.s.roll);
    // start from the column end nearer where the roll is (upright after a pan)
    if (Math.abs(wrap180(col[col.length - 1].s.roll - rollAt)) < Math.abs(wrap180(col[0].s.roll - rollAt))) col.reverse();
    for (const c of col) steps.push({ ...c, col: i });
    rollAt = order.length > i + 1 && Math.abs(order[i + 1] - k) > 3 ? hr : col[col.length - 1].s.roll;
  }
  let travel = 0;
  for (let i = 1; i < steps.length; i++) travel += Math.abs(steps[i].s.turnYaw - steps[i - 1].s.turnYaw) + Math.abs(wrap180(steps[i].s.roll - steps[i - 1].s.roll));
  return { steps, unreachable, travel, columns: order.length };
}

/** A precalculated move: turn at these rates (°/s: pan = yaw, tilt = pitch, roll) for `ms`; `shot` fires the X1 at
 * planned shot `shot` when the move is done; `laser` turns it on before the move (it warms up while the gimbal
 * turns). */
export interface Op {
  yaw: number;
  pitch: number;
  roll: number;
  ms: number;
  laser?: boolean;
  shot?: number;
}

/** Timed turns come up about 2° short, big or small (bench 2026-11-17: 5→3.3, 10→9.3, 20→18, 45→41.4, 80→77.3). */
export const MOVE_SHORTFALL = 2;

/** The whole scan as moves, worked out in advance and played as they stand (no reckoning while it runs): from home
 * (pan centred, upright) to each shot in plan order; between columns the clamp stands upright first, then pans, then
 * rolls; roll and pan at `rate`, each move lengthened for the shortfall; back home at the end. */
export function compile(p: ReturnType<typeof plan>, home: Att, rate = 30): Op[] {
  const hr = euler(home.q)[0];
  const ops: Op[] = [];
  let yaw = 0, roll = hr;
  const turn = (axis: 'yaw' | 'roll', deg: number, extra: Partial<Op> = {}) => {
    if (Math.abs(deg) < 0.3) return false;
    ops.push({ yaw: 0, pitch: 0, roll: 0, [axis]: Math.sign(deg) * rate, ms: ((Math.abs(deg) + MOVE_SHORTFALL) / rate) * 1000, ...extra });
    return true;
  };
  for (const [i, st] of p.steps.entries()) {
    const dy = st.s.turnYaw - yaw;
    if (Math.abs(dy) > 3) {
      turn('roll', wrap180(hr - roll));
      roll = hr;
      turn('yaw', dy);
      yaw = st.s.turnYaw;
    }
    const moved = turn('roll', wrap180(st.s.roll - roll), { laser: true });
    roll = st.s.roll;
    if (moved) ops[ops.length - 1].shot = i;
    else ops.push({ yaw: 0, pitch: 0, roll: 0, ms: 0, laser: true, shot: i });
  }
  turn('roll', wrap180(hr - roll));
  turn('yaw', -yaw);
  return ops;
}

/** How long a program takes: its moves, a settle after each shot's move, and the shots (~1.5 s with the laser warmed
 * during the move). */
export function programSeconds(ops: Op[], o: { shotS: number; settleS: number }) {
  return ops.reduce((t, op) => t + op.ms / 1000 + (op.shot !== undefined ? o.shotS + o.settleS : 0), 0);
}
