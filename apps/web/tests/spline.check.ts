import { simplify, smooth, fitStroke, nearestPoint, distanceToCurve, insertionAt, refit, detailFraction } from '../src/lib/spline.ts';
let bad = 0; const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
// a dense arc simplifies to a handful of points that stay within tolerance
const arc = Array.from({ length: 200 }, (_, i) => [i, Math.sin(i / 200 * Math.PI) * 40] as [number, number]);
const s = simplify(arc, arc.map(() => 1), 0.8);
ok(s.pts.length < 30 && s.pts.length >= 4, `fit points ${s.pts.length}`);
ok(s.pts[0] === arc[0] && s.pts[s.pts.length-1] === arc[199], 'ends kept');
ok(s.widths!.length === s.pts.length, 'widths follow');
for (const p of arc) ok(distanceToCurve(s.pts, p[0], p[1]) < 1.5, 'curve stays near the drawing');
// a straight line collapses to its ends
ok(simplify([[0,0],[1,1],[2,2],[3,3]], null, 0.1).pts.length === 2, 'straight is two points');
// the spline passes through every fit point
const fit: [number,number][] = [[0,0],[10,5],[20,0],[30,8]];
const c = smooth(fit, null, 10).pts;
for (const f of fit) ok(c.some(p => Math.hypot(p[0]-f[0], p[1]-f[1]) < 1e-9), `passes through ${f}`);
ok(smooth([[0,0],[5,5]]).pts.length === 2, 'two points stay straight');
ok(nearestPoint(fit, 11, 4, 3) === 1 && nearestPoint(fit, 100, 100, 3) === -1, 'nearest point');
ok(insertionAt(fit, 15, 4).index === 2, 'insertion span');
// duplicated points do not break the spline
ok(smooth([[0,0],[0,0],[5,5],[10,0]]).pts.every(p => Number.isFinite(p[0]) && Number.isFinite(p[1])), 'finite with duplicates');
// a closed curve returns to its start and passes through every fit point
const loop: [number,number][] = [[0,0],[20,0],[20,20],[0,20]];
const lc = smooth(loop, null, 10, true).pts;
ok(Math.hypot(lc[lc.length-1][0]-lc[0][0], lc[lc.length-1][1]-lc[0][1]) < 1e-9, 'a closed curve ends where it began');
for (const f of loop) ok(lc.some(p => Math.hypot(p[0]-f[0], p[1]-f[1]) < 1e-9), `closed passes through ${f}`);
ok(lc.length === 4*10 + 1, `four spans, got ${lc.length}`);
ok(distanceToCurve(loop, 10, 0, true) < 3 && distanceToCurve(loop, 0, 10, true) < 3, 'the closing span is part of the curve');
ok(distanceToCurve(loop, 0, 10, false) > 5, 'an open curve has no closing span');
ok(insertionAt(loop, 0, 10, true).index === 4, 'a point added on the closing span goes at the end');
ok(fitStroke([[0,0],[10,0],[20,0],[20,10],[20,20],[10,20],[0,20],[0,10],[0,1]], null, 1, true).pts.length === 4, 'closed fit drops the point beside the start');
// re-fitting a drawn line at another detail
const wave: [number, number][] = Array.from({ length: 14 }, (_, i) => [i * 10, Math.sin(i / 2) * 20]);
const lo = refit(wave, wave.map(() => 1), false, 1), hi = refit(wave, wave.map(() => 1), false, 10), mid = refit(wave, null, false, 5);
ok(lo.pts.length < hi.pts.length, `low detail has fewer points (${lo.pts.length} vs ${hi.pts.length})`);
ok(lo.pts.length <= mid.pts.length && mid.pts.length <= hi.pts.length, 'detail only ever adds points');
ok(lo.pts[0][0] === 0 && hi.pts[hi.pts.length - 1][0] === 130, 'the ends stay');
ok(lo.widths!.length === lo.pts.length && mid.widths === null, 'pen weights follow the points, and stay absent when there were none');
const curveOf = (p: [number, number][], closed = false) => smooth(p, null, 10, closed).pts;
const original = curveOf(wave);
const refitted = curveOf(hi.pts);
let worst = 0; for (const q of original) worst = Math.max(worst, distanceToCurve(hi.pts, q[0], q[1]));
ok(worst < 130 * 1.2 * detailFraction(10) * 3, `detail 10 stays on the curve (off by ${worst.toFixed(3)})`);
void refitted;
ok(refit([[0,0],[5,5]], null, false, 5).pts.length === 2, 'a two point line is left alone');
const ring: [number, number][] = Array.from({ length: 12 }, (_, i) => [Math.cos(i / 12 * 2 * Math.PI) * 50, Math.sin(i / 12 * 2 * Math.PI) * 50]);
const rl = refit(ring, null, true, 2), rh = refit(ring, null, true, 9);
ok(rl.pts.length >= 3 && rl.pts.length <= rh.pts.length, `a closed loop keeps at least three points (${rl.pts.length}, ${rh.pts.length})`);
ok(Math.hypot(rh.pts[0][0] - rh.pts[rh.pts.length - 1][0], rh.pts[0][1] - rh.pts[rh.pts.length - 1][1]) > 1e-6, 'a closed loop does not repeat its first point');
ok(detailFraction(1) > detailFraction(5) && detailFraction(5) > detailFraction(10) && detailFraction(0) === detailFraction(1) && detailFraction(99) === detailFraction(10), 'detail runs from loose to tight, clamped');
console.log(bad ? 'FAILED' : 'spline checks ok'); process.exit(bad ? 1 : 0);
