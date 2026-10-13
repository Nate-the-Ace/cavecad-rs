import { fitPlane, heightSurface } from '../src/lib/plane.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const near = (a: number, b: number) => Math.abs(a - b) < 1e-6;
const at = (pl: [number, number, number], x: number, y: number) => pl[0] + pl[1] * x + pl[2] * y;

const plane = (x: number, y: number) => 10 + 0.2 * x - 0.1 * y;
const pts: [number, number, number][] = [[0, 0], [10, 0], [0, 10], [7, 9], [-5, 4]].map(([x, y]) => [x, y, plane(x, y)]);
const p = fitPlane(pts, 99);
ok(pts.every((q) => near(at(p, q[0], q[1]), q[2])), 'stations on a plane are touched exactly');
ok(near(at(p, 3, 3), plane(3, 3)), 'and the plane carries on between them');

ok(fitPlane([], 5).join() === '5,0,0', 'no stations: level at the fallback height');
const one = fitPlane([[4, 4, 7]], 0);
ok(near(at(one, 4, 4), 7) && one[1] === 0 && one[2] === 0, 'one station: level through it');

const line = fitPlane([[0, 0, 0], [10, 0, 2], [20, 0, 4]], 0);
ok(near(at(line, 10, 0), 2) && near(at(line, 10, 50), 2), 'stations in a line: sloping along it, level across');
const diag = fitPlane([[0, 0, 0], [10, 10, 5], [20, 20, 10]], 0);
ok(near(at(diag, 20, 20), 10) && near(at(diag, 10, 10), 5) && near(at(diag, 10, 0), at(diag, 0, 10)), 'a diagonal line works the same');

const shaft = fitPlane([[0, 0, 0], [1, 0, -50], [2, 0, -100]], 0);
ok(Math.hypot(shaft[1], shaft[2]) <= 1 + 1e-9, 'a shaft cannot stand the scan on its edge');

// the height surface: exact at every station, smooth between, never beyond a little past their range
const rolling: [number, number, number][] = [[0, 0, 0], [20, 0, 4], [40, 5, 1], [60, -3, 8], [30, 20, 3], [10, 25, 6]];
const h = heightSurface(rolling, 0);
ok(rolling.every((q) => Math.abs(h(q[0], q[1]) - q[2]) < 1e-3), 'the surface passes through every station');
const mid = h(30, 3);
ok(mid > -2 && mid < 10, 'and between stations it stays among their heights: ' + mid);
const far = h(5000, 5000);
ok(far <= 8 + 1.6 + 1e-9 && far >= 0 - 1.6 - 1e-9, 'far away it is held near their range: ' + far);
const same = heightSurface([[0, 0, 0], [0, 0, 10], [10, 0, 2], [0, 10, 4]], 0);
ok(Number.isFinite(same(3, 3)) && near(same(0, 0), 5), 'stations on top of each other are averaged');
const two = heightSurface([[0, 0, 0], [10, 0, 5]], 0);
ok(near(two(5, 0), 2.5), 'two stations: a slope between them');

if (bad) process.exit(1);
console.log('plane ok');
