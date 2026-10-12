import { simplify, smooth, nearestPoint, distanceToCurve, insertionAt } from '../src/lib/spline.ts';
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
console.log(bad ? 'FAILED' : 'spline checks ok'); process.exit(bad ? 1 : 0);
