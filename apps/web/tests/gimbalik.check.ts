// Gimbal kinematics (attitude model): every answer points where asked, stays in the safe windows, and goes round the
// other way where the short way would flip the roll. Same cases as the Rust library's aim tests.
import { solutions, pointing, fromEuler, euler, homeFrom, plan, compile, programSeconds, panRollDrag, ringDirections, domeDirections, clampTop, OM7_PORTRAIT, wrap180, type Att } from '../src/lib/instrument/gimbal-ik.ts';

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
  const deep = plan(ringDirections(0, 10, -30), home);
  ok(deep.unreachable.length > 0 && deep.unreachable.every((d) => d.inc < -15), `deep ring misses only the deep ones: ${JSON.stringify(deep.unreachable)}`);
}
// facing a wall cannot work: the pan stops (+84 / -198) cannot face behind
ok(plan(ringDirections(-90, 15, 0), home).unreachable.length > 0, 'facing a wall misses the back wall');
// dome: straight up first, most of it reachable
{
  const all = domeDirections(15);
  const p = plan(all, home);
  ok(p.unreachable.length < all.length / 3, `dome reaches most: ${p.unreachable.length} of ${all.length} missed`);
}

// columns: inside one the yaw never changes and the roll moves one step at a time; the ring is two columns (the
// right wall with the top and the high left wall, then the left wall from behind)
{
  const p = plan(ringDirections(0, 10, -20), home);
  ok(p.columns === 2, `ring columns ${p.columns}`);
  for (let i = 1; i < p.steps.length; i++)
    if (p.steps[i].col === p.steps[i - 1].col) {
      ok(Math.abs(p.steps[i].s.turnYaw - p.steps[i - 1].s.turnYaw) < 0.2, 'same yaw in a column');
      ok(Math.abs(wrap180(p.steps[i].s.roll - p.steps[i - 1].s.roll)) <= 10.01, `one step of roll: ${p.steps[i - 1].s.roll} -> ${p.steps[i].s.roll}`);
    }
  // the program: replaying its moves (with the shortfall taken back off) lands on every shot and ends home
  const ops = compile(p, home);
  let yaw = 0, roll = -90;
  const seen: number[] = [];
  for (const op of ops) {
    const deg = Math.max(0, (op.ms / 1000) * 30 - 2);
    yaw += Math.sign(op.yaw) * (op.yaw ? deg : 0);
    roll += Math.sign(op.roll) * (op.roll ? deg : 0);
    // the roll joint (with the pan's drag) never passes its window on the way
    const rj = 95.7 + wrap180(roll + 90) + panRollDrag(yaw);
    ok(rj >= OM7_PORTRAIT.roll[0] - 3 && rj <= OM7_PORTRAIT.roll[1] + 0.01, `roll joint inside on the way: ${rj} at yaw ${yaw}`);
    if (op.shot !== undefined) {
      const s = p.steps[op.shot].s;
      ok(Math.abs(yaw - s.turnYaw) < 0.01 && Math.abs(wrap180(roll - s.roll)) < 0.01, `shot ${op.shot} at yaw ${yaw} roll ${roll}, planned ${s.turnYaw} ${s.roll}`);
      seen.push(op.shot);
    }
  }
  ok(seen.length === p.steps.length, 'every shot once');
  ok(Math.abs(yaw) < 0.01 && Math.abs(roll + 90) < 0.01, `ends home: ${yaw} ${roll}`);
  const secs = programSeconds(ops, { shotS: 1.5, settleS: 0.3 });
  ok(secs > p.steps.length * 1.8 && secs < p.steps.length * 4, `ring time ${secs}`);
  const t0 = Date.now();
  compile(plan(domeDirections(4), home), home);
  ok(Date.now() - t0 < 300, `dome 4° plans fast: ${Date.now() - t0} ms`);
}

if (fails) {
  console.error(`${fails} gimbal kinematics check(s) failed`);
  process.exit(1);
}
console.log('gimbal kinematics: ok');
