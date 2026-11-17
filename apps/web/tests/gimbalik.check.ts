// Gimbal kinematics (attitude model): every answer points where asked, stays in the safe windows, and goes round the
// other way where the short way would flip the roll. Same cases as the Rust library's aim tests.
import { solutions, pointing, fromEuler, euler, homeFrom, plan, compile, compileLists, LIST_MAX, programSeconds, targetPointing, targetSolutions, OM7_TARGETS, ringDirections, domeDirections, clampTop, OM7_PORTRAIT, wrap180, type Att } from '../src/lib/instrument/gimbal-ik.ts';

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

// ---- target mode: the model against the bench (sag 15.8, X1 aboard)
{
  const bench: [number, number, number, number][] = [
    // target yaw, roll -> measured az, inc
    [0, 0, 180, 74.1],
    [0, 45, -105.6, 42.9],
    [0, 90, -90.3, -0.5],
    [0, 110, -84.9, -19.1],
    [-180, 90, 90.6, 1.2],
    [-180, 110, 85.4, -17.7],
    [-180, 45, 104.7, 44.5],
  ];
  for (const [y, r, az, inc] of bench) {
    const g = targetPointing({ yaw: y, roll: r }, 15.8);
    ok(Math.abs(wrap180(g.az - az)) < 2 && Math.abs(g.inc - inc) < 2, `target ${y}/${r}: model ${g.az.toFixed(1)}/${g.inc.toFixed(1)}, bench ${az}/${inc}`);
  }
  // inverse: every answer points back where asked, inside the reach
  for (let az = -180; az < 180; az += 15)
    for (let inc = -20; inc <= 90; inc += 10)
      for (const t of targetSolutions({ az, inc }, 16)) {
        const g = targetPointing(t, 16);
        ok((inc > 89.9 || Math.abs(wrap180(g.az - az)) < 0.01) && Math.abs(g.inc - inc) < 0.01, `inverse ${az}/${inc} -> ${JSON.stringify(t)} -> ${JSON.stringify(g)}`);
        ok(t.roll >= OM7_TARGETS.roll[0] && t.roll <= OM7_TARGETS.roll[1], 'roll in reach');
      }
  // straight up is reachable: lean out the sag with roll
  ok(targetSolutions({ az: 0, inc: 90 }, 16).length > 0, 'straight up');
  // both walls level and 20 below: reachable (left = yaw 0 roll + ; right = yaw -180 roll +, or the short side)
  for (const az of [90, -90]) for (const inc of [0, -10, -18]) ok(targetSolutions({ az, inc }, 16).length > 0, `wall ${az} at ${inc}`);
  // a ring facing the forward station, every 10 to 18 below: all reachable; program steps never jump more than 20
  const p = plan(ringDirections(0, 10, -18), 16);
  ok(p.unreachable.length === 0, `ring: ${JSON.stringify(p.unreachable)}`);
  const lists = compileLists(p);
  const prog = lists.flat();
  for (const l of lists) {
    ok(l.length <= LIST_MAX, `list of ${l.length}`);
    const last = l[l.length - 1].t;
    ok(Math.abs(last.yaw) < 0.01 && Math.abs(last.roll) < 0.01, 'every list ends home');
    let at = { yaw: 0, roll: 0 };
    for (const st of l) {
      // legs up to ~67° (the farthest, 200° of yaw, in 3), at most 45 between shots; each move timed at 15°/s
      ok(Math.abs(st.t.yaw - at.yaw) <= 70 && Math.abs(st.t.roll - at.roll) <= 70 && st.ms >= 600 && st.ms <= 15000 && st.t.yaw >= OM7_TARGETS.yaw[0] && st.t.yaw <= OM7_TARGETS.yaw[1], `step ${JSON.stringify(at)} -> ${JSON.stringify(st.t)}`);
      at = st.t;
    }
  }
  ok(prog.filter((st) => st.shot !== undefined).length === p.steps.length, 'every shot once');
  const secs = programSeconds(prog, { arriveS: 0.5, shotS: 1.5, settleS: 0.3 });
  ok(secs > p.steps.length * 2.5 && secs < p.steps.length * 7, `ring ${p.steps.length} shots in ${secs} s`);
  // the dome: most reachable, plans fast
  const t0 = Date.now();
  const dp = plan(domeDirections(5), 16);
  const dl = compileLists(dp);
  ok(dl.every((l) => l.length <= LIST_MAX), 'dome lists fit');
  console.log(`  ring 15°: ${p.steps.length} shots in ${lists.length} lists; dome 5°: ${dp.steps.length} shots in ${dl.length} lists, ${dl.flat().length} targets`);
  ok(Date.now() - t0 < 300, `dome 5° plans fast: ${Date.now() - t0} ms`);
  ok(dp.unreachable.length < domeDirections(5).length / 5, `dome reach: ${dp.unreachable.length} missed`);
}

if (fails) {
  console.error(`${fails} gimbal kinematics check(s) failed`);
  process.exit(1);
}
console.log('gimbal kinematics: ok');
