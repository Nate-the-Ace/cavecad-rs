// Gimbal kinematics (attitude model): every answer points where asked, stays in the safe windows, and goes round the
// other way where the short way would flip the roll. Same cases as the Rust library's aim tests.
import { solutions, pointing, fromEuler, euler, homeFrom, plan, ringDirections, domeDirections, clampTop, OM7_PORTRAIT, wrap180, type Att } from '../src/lib/instrument/gimbal-ik.ts';

let fails = 0;
const ok = (c: boolean, msg: string) => {
  if (!c) {
    fails++;
    console.error('FAIL', msg);
  }
};
const home: Att = { q: fromEuler(-90, 0, -4), pan: 0, tilt: 30, roll: 95.7 };
const at = (roll: number, yaw: number): Att => ({ q: fromEuler(roll, 0, yaw), pan: 0, tilt: 30, roll: 95.7 + roll + 90 });

// euler round trip
{
  const [r, p, y] = euler(fromEuler(-63, 2, 37));
  ok(Math.abs(r + 63) < 1e-6 && Math.abs(p - 2) < 1e-6 && Math.abs(y - 37) < 1e-6, `euler ${r} ${p} ${y}`);
}
// the bench quaternion at rest points up; rolled +27 it leans
ok(Math.abs(clampTop([0.7067, -0.7064, 0.0354, -0.016]).inc - 88.5) < 1, 'bench rest');
ok(Math.abs(clampTop([0.8044, -0.5005, -0.1525, 0.2815]).inc - 63) < 1, 'bench rolled');

for (let az = -180; az < 180; az += 15)
  for (let inc = -30; inc <= 90; inc += 10)
    for (const s of solutions({ az, inc }, home, home)) {
      const g = pointing(at(s.roll, -4 + s.turnYaw), home);
      ok(Math.abs(g.inc - inc) < 0.01 && (inc > 89 || Math.abs(wrap180(g.az - az)) < 0.01), `az ${az} inc ${inc}: ${JSON.stringify(s)} -> ${JSON.stringify(g)}`);
      ok(s.panJoint >= OM7_PORTRAIT.pan[0] && s.panJoint <= OM7_PORTRAIT.pan[1] && s.rollJoint >= OM7_PORTRAIT.roll[0] && s.rollJoint <= OM7_PORTRAIT.roll[1], 'inside windows');
    }

const left = solutions({ az: -90, inc: 0 }, home, home)[0];
ok(!!left && Math.abs(left.turnYaw + 180) < 0.01 && Math.abs(left.roll) < 0.01, `left wall goes round: ${JSON.stringify(left)}`);
const right = solutions({ az: 90, inc: -10 }, home, home)[0];
ok(!!right && Math.abs(right.turnYaw) < 0.01 && Math.abs(right.roll - 10) < 0.01, `right wall rolls: ${JSON.stringify(right)}`);
ok(solutions({ az: 90, inc: -40 }, home, home).length === 0, 'too far below');
ok(solutions({ az: -150, inc: 10 }, home, home).length === 0, 'behind-left low');

// home from anywhere: panned 40 and rolled 30 from home gives back home's heading and joints
{
  const a: Att = { q: fromEuler(-60, 0, 36), pan: 40, tilt: 25, roll: 125.7 };
  const h = homeFrom(a);
  ok(Math.abs(wrap180(euler(h.q)[2] + 4)) < 0.01 && Math.abs(euler(h.q)[0] + 90) < 0.01 && Math.abs(h.roll - 95.7) < 0.01, `homeFrom ${JSON.stringify(h)}`);
}

// a ring with the gimbal facing the forward station: both walls (az 90 and -90) reachable to 15 below level, the left
// one by going round; deeper than ~19 below on either side is past the roll window
{
  const p = plan(ringDirections(0, 15, -15), home);
  ok(p.unreachable.length === 0, `ring: ${JSON.stringify(p.unreachable)}`);
  ok(p.steps[0].d.inc >= 89.5, 'ring starts straight up');
  const deep = plan(ringDirections(0, 10, -30), home);
  ok(deep.unreachable.length > 0 && deep.unreachable.every((d) => d.inc < -15), `deep ring misses only the deep ones: ${JSON.stringify(deep.unreachable)}`);
}
// facing a wall cannot work: the pan stops (+84 / -198) cannot face behind
ok(plan(ringDirections(-90, 15, 0), home).unreachable.length > 0, 'facing a wall misses the back wall');
// dome: straight up first, most of it reachable
{
  const all = domeDirections(15);
  const p = plan(all, home);
  ok(p.steps[0].d.inc >= 89.5, 'dome starts up');
  ok(p.unreachable.length < all.length / 3, `dome reaches most: ${p.unreachable.length} of ${all.length} missed`);
}

if (fails) {
  console.error(`${fails} gimbal kinematics check(s) failed`);
  process.exit(1);
}
console.log('gimbal kinematics: ok');
