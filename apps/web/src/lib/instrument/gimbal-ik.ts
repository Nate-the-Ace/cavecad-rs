// Gimbal kinematics for an X1 riding a phone gimbal with its laser out of the clamp's top (tools/cave-osmo/FINDINGS.md).
//
// Joints, from the gimbal's own centre (where it was set down, upright, front forward):
//   pan  ψ: turns everything about the vertical; + is clockwise seen from above (to the right).
//   roll ρ: swings the clamp about the camera (front) axis; the laser, straight up at ρ = 0, leans sideways by |ρ|.
//           + leans it to the side `rightIsPositive` says (fixed by how the X1 sits in the clamp).
//   tilt θ: nods the clamp about the side axis; held at 0 except to cancel a fore/aft lean.
//
// Forward kinematics (tilt 0): the laser's inclination is 90 − |ρ| (below level past 90) and its azimuth, from the
// gimbal's front, is ψ + 90° toward the side ρ leans. Inverse: for a wanted direction there are two answers, one per
// roll side (ρ = ±(90 − inc), ψ = az ∓ 90); keep the ones inside the reach and pick the cheapest from where it is.
// Everything is in degrees.

export interface Joints {
  pan: number;
  roll: number;
  tilt: number;
}

/** A laser direction relative to the gimbal's front: azimuth clockwise from the front, inclination up from level. */
export interface Direction {
  az: number;
  inc: number;
}

export interface Reach {
  /** Pan either way from the centre. */
  pan: number;
  /** Roll on the long side and the short side (which way is long: `longRoll`, +1 or −1; 0 unknown = both short). */
  rollLong: number;
  rollShort: number;
  longRoll: number;
  /** Which side + roll leans the laser to, seen looking out from the gimbal's front. */
  rightIsPositive: boolean;
}

/** Measured on the OM7 with the X1 aboard, less a margin (FINDINGS). */
export const OM7_REACH: Reach = { pan: 130, rollLong: 120, rollShort: 35, longRoll: 0, rightIsPositive: true };

export const wrap180 = (a: number) => {
  let x = ((a % 360) + 360) % 360;
  if (x > 180) x -= 360;
  return x;
};

/** Where the laser points for these joints (tilt taken as 0). */
export function forward(j: Joints, reach: Pick<Reach, 'rightIsPositive'> = OM7_REACH): Direction {
  const side = (reach.rightIsPositive ? 1 : -1) * Math.sign(j.roll || 0);
  return { az: wrap180(j.pan + (side >= 0 ? 90 : -90)), inc: 90 - Math.abs(j.roll) };
}

const rollLimit = (sign: number, r: Reach) => (r.longRoll === 0 ? r.rollShort : sign === r.longRoll ? r.rollLong : r.rollShort);

/** Every way to point at `d` inside the reach (0, 1 or 2 answers; one for straight up). */
export function solutions(d: Direction, reach: Reach = OM7_REACH): Joints[] {
  const lean = 90 - d.inc; // how far from straight up
  if (lean < 0 || lean > 180) return [];
  if (lean < 0.5) return [{ pan: 0, roll: 0, tilt: 0 }];
  const out: Joints[] = [];
  for (const sign of [1, -1]) {
    const roll = sign * lean;
    if (Math.abs(roll) > rollLimit(sign, reach)) continue;
    const side = (reach.rightIsPositive ? 1 : -1) * sign;
    const pan = wrap180(d.az - side * 90);
    if (Math.abs(pan) > reach.pan) continue;
    out.push({ pan, roll, tilt: 0 });
  }
  return out;
}

/** Cost of moving between two joint sets: the longest single move (the axes turn one after another at one rate). */
export const cost = (a: Joints, b: Joints) => Math.abs(a.pan - b.pan) + Math.abs(a.roll - b.roll);

/** The cheapest way to point at `d` from `from`, or null if the gimbal cannot reach it. */
export function solve(d: Direction, from: Joints, reach: Reach = OM7_REACH): Joints | null {
  const s = solutions(d, reach);
  if (!s.length) return null;
  return s.reduce((best, j) => (cost(from, j) < cost(from, best) ? j : best));
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

/** Orders reachable targets so the gimbal moves little between them: by pan, rolling down then up in turn (zig-zag),
 * and says which were out of reach. */
export function plan(dirs: Direction[], reach: Reach = OM7_REACH, from: Joints = { pan: 0, roll: 0, tilt: 0 }) {
  const reachable: { d: Direction; j: Joints }[] = [];
  const unreachable: Direction[] = [];
  let at = from;
  // solve each against a neutral start first, then order; ties broken toward the side already in use
  for (const d of dirs) {
    const j = solve(d, at, reach);
    if (j) reachable.push({ d, j });
    else unreachable.push(d);
  }
  const panKey = (p: number) => Math.round(p * 10) / 10;
  const byPan = new Map<number, { d: Direction; j: Joints }[]>();
  for (const r of reachable) {
    const k = panKey(r.j.pan);
    if (!byPan.has(k)) byPan.set(k, []);
    byPan.get(k)!.push(r);
  }
  const ordered: { d: Direction; j: Joints }[] = [];
  let down = true;
  for (const k of [...byPan.keys()].sort((a, b) => a - b)) {
    const col = byPan.get(k)!.sort((a, b) => Math.abs(a.j.roll) - Math.abs(b.j.roll) || a.j.roll - b.j.roll);
    ordered.push(...(down ? col : col.reverse()));
    down = !down;
  }
  // straight up first if it is in the list (it is the reference shot)
  const upIdx = ordered.findIndex((o) => o.d.inc >= 89.5);
  if (upIdx > 0) ordered.unshift(...ordered.splice(upIdx, 1));
  at = from;
  return { steps: ordered, unreachable };
}
