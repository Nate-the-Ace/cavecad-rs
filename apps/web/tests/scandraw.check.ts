import { screenMatrix } from '../src/lib/scan-draw.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const apply = (m: number[], x: number, y: number): [number, number] => [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]];

// A scan pixel goes to the view's metres (x = 0.01 px + 3, y = -0.01 py + 5: north up the page), then to the screen.
const matrix: [number, number, number, number, number, number] = [0.01, 0, 3, 0, -0.01, 5];
const frame = { ox: 100, oy: 400, scale: 20 };
const [sx, sy] = apply(screenMatrix(matrix, frame), 200, 100);
const X = 0.01 * 200 + 3;
const Y = -0.01 * 100 + 5;
ok(Math.abs(sx - (100 + 20 * X)) < 1e-9 && Math.abs(sy - (400 - 20 * Y)) < 1e-9, `metres to screen: ${sx},${sy}`);

// A scan turned a quarter (x = -0.02 py, y = 0.02 px): its pixel (50, 10) is at metres (-0.2, 1).
const [tx, ty] = apply(screenMatrix([0, -0.02, 0, 0.02, 0, 0], { ox: 0, oy: 0, scale: 1 }), 50, 10);
ok(Math.abs(tx - -0.2) < 1e-9 && Math.abs(ty - -1) < 1e-9, `turned scan: ${tx},${ty}`);

if (bad) process.exit(1);
console.log('scan draw ok');
