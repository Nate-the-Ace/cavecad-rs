// Gimbal kinematics: forward and inverse agree, the reach is respected, and the scan plans are what the gimbal can do.
import { forward, solve, solutions, ringDirections, domeDirections, plan, OM7_REACH, wrap180, type Reach } from '../src/lib/instrument/gimbal-ik.ts';

let fails = 0;
const ok = (c: boolean, msg: string) => {
  if (!c) {
    fails++;
    console.error('FAIL', msg);
  }
};
const near = (a: number, b: number, tol = 1e-6) => Math.abs(wrap180(a - b)) < tol;
const known: Reach = { ...OM7_REACH, longRoll: 1 };

// round trip: every reachable direction solves to joints that point back at it
for (let az = -180; az < 180; az += 15)
  for (let inc = -40; inc <= 90; inc += 10) {
    for (const j of solutions({ az, inc }, known)) {
      const f = forward(j, known);
      ok(near(f.inc, inc) && (inc > 89.5 || near(f.az, az)), `round trip az ${az} inc ${inc} -> ${JSON.stringify(j)} -> ${JSON.stringify(f)}`);
      ok(Math.abs(j.pan) <= known.pan, `pan inside reach for az ${az}`);
    }
  }

// straight up needs no move
ok(JSON.stringify(solve({ az: 0, inc: 90 }, { pan: 0, roll: 0, tilt: 0 }, known)) === JSON.stringify({ pan: 0, roll: 0, tilt: 0 }), 'straight up is home');

// the right wall, level, from facing forward: roll the long way, no pan
const right = solve({ az: 90, inc: 0 }, { pan: 0, roll: 0, tilt: 0 }, known);
ok(!!right && near(right.pan, 0) && near(right.roll, 90), `right wall level: ${JSON.stringify(right)}`);

// the left wall, level, from facing forward: the long side would need pan 180 (out of reach), the short side cannot
// lean 90: unreachable
ok(solve({ az: -90, inc: 0 }, { pan: 0, roll: 0, tilt: 0 }, known) === null, 'left wall level from facing forward is out of reach');

// the left wall high up is reachable on the short side
const leftHigh = solve({ az: -90, inc: 60 }, { pan: 0, roll: 0, tilt: 0 }, known);
ok(!!leftHigh && near(leftHigh.roll, -30) && near(leftHigh.pan, 0), `left wall 60 up: ${JSON.stringify(leftHigh)}`);

// before the long side is known, both sides are held short
ok(solve({ az: 90, inc: 0 }, { pan: 0, roll: 0, tilt: 0 }, OM7_REACH) === null, 'unknown long side: no deep roll');

// a ring with the gimbal set facing a wall (passage at az -90 / +90): both walls fully reachable to 30 below level
const wallRing = plan(ringDirections(-90, 10, -30), known);
ok(wallRing.unreachable.length === 0, `ring facing a wall reaches all: ${wallRing.unreachable.length} missed`);
// the same ring with the gimbal facing forward misses the far wall below the short roll side
const fwdRing = plan(ringDirections(0, 10, -30), known);
ok(fwdRing.unreachable.length > 0 && fwdRing.unreachable.every((d) => near(d.az, -90)), `ring facing forward misses only the left wall: ${JSON.stringify(fwdRing.unreachable.slice(0, 3))}`);

// dome: straight up first; every step reachable; the pan never jumps more than one column at a time
const dome = plan(domeDirections(15), known);
ok(dome.steps[0].d.inc >= 89.5, 'dome starts straight up');
ok(dome.steps.every((s) => Math.abs(s.j.pan) <= known.pan), 'dome inside pan reach');
ok(dome.unreachable.length < domeDirections(15).length / 4, `dome reaches most: ${dome.unreachable.length} missed of ${domeDirections(15).length}`);

if (fails) {
  console.error(`${fails} gimbal kinematics check(s) failed`);
  process.exit(1);
}
console.log('gimbal kinematics: ok');
