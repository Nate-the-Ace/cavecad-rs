// Checks the SVG symbol importer against a made-up set: transforms, curves, arcs, fills.
import { normalise, parsePath, parseTransform, readSvg } from './import_symbol_set.mjs';
const eq = (a, b, what) => { if (JSON.stringify(a) !== JSON.stringify(b)) throw new Error(`${what}: got ${JSON.stringify(a)}, wanted ${JSON.stringify(b)}`); };
const near = (a, b, what, tol = 1e-3) => { if (Math.abs(a - b) > tol) throw new Error(`${what}: got ${a}, wanted ${b}`); };

eq(parseTransform('translate(10 5) scale(2)'), [2, 0, 0, 2, 10, 5], 'transforms compose left to right');
const sq = parsePath('M0 0 H10 v10 h-10 z');
eq(sq[0], { pts: [[0, 0], [10, 0], [10, 10], [0, 10]], closed: true }, 'H, v, h and z');
const arcs = parsePath('M10 0 A10 10 0 0 1 -10 0');
near(Math.max(...arcs[0].pts.map((p) => p[1])), 10, 'a half circle arc reaches its top');
const curve = parsePath('M0 0 C0 10 10 10 10 0');
near(curve[0].pts[curve[0].pts.length - 1][0], 10, 'a cubic ends where it says');

const svg = `<svg viewBox="0 0 100 100"><g transform="translate(50 50)"><circle cx="0" cy="0" r="20" fill="none" stroke="#000"/><path d="M0 0 L0 -40" stroke="#000"/><rect x="-5" y="10" width="10" height="10" fill="#000" stroke="none"/></g></svg>`;
const shapes = readSvg(svg);
eq(shapes.length, 3, 'a circle, a line and a rectangle are read');
eq(shapes.filter((s) => s.fill).length, 1, 'only the solid rectangle is filled');
const prims = normalise(shapes);
const far = Math.max(...prims.flatMap((p) => p.pts.map((q) => Math.hypot(q[0], q[1]))));
near(far, 1, 'the drawing is scaled so its farthest point is at distance 1');
const line = prims.find((p) => p.pts.length === 2);
if (!(line.pts[1][1] > line.pts[0][1])) throw new Error('the SVG line drawn upward stays upward (y is flipped to point up)');
console.log('symbol importer checks ok');
