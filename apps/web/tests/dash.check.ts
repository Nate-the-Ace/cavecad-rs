import { DOT_MM, dashMetres, dashPx } from '../src/lib/dash.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };

const dashed = [2.0, -1.2];
ok(dashPx([], 10) .length === 0, 'solid has no dashes');
const a = dashPx(dashed, 4);
ok(a.length === 2 && a[0] === 8 && Math.abs(a[1] - 4.8) < 1e-9, 'a dash and a gap are millimetres times pixels per millimetre: ' + a);
const b = dashPx(dashed, 4, 2);
ok(b[0] === 16 && Math.abs(b[1] - 9.6) < 1e-9, 'a line\'s own scale lengthens both');
ok(dashPx(dashed, 0.5).length === 0, 'too fine to see, drawn solid: ' + dashPx(dashed, 0.5));
ok(dashPx(dashed, 4, 0).length === 0 && dashPx(dashed, NaN).length === 0, 'nonsense is solid');
const dd = dashPx([2.8, -1.2, 0, -1.2], 5);
ok(dd.length === 4 && Math.abs(dd[2] - DOT_MM * 5) < 1e-9, 'a dot is drawn as a short dash');
ok(dashPx([3, -1, 2], 5).length === 6, 'an odd pattern repeats itself so dashes and gaps alternate');
ok(dashPx([1000, -1], 1000)[0] <= 4000, 'a huge magnification is held to a size a canvas draws');

// the same pattern at two drawing scales: the same on paper, different on the ground
const at500 = dashMetres(dashed, 500)!;
const at2000 = dashMetres(dashed, 2000)!;
ok(Math.abs(at500.dash - 1.0) < 1e-9 && Math.abs(at500.gap - 0.6) < 1e-9, '2 mm at 1:500 is a metre of ground');
ok(Math.abs(at2000.dash / at500.dash - 4) < 1e-9, 'at 1:2000 the same dash is four times as much ground');
ok(dashMetres([], 500) === null && dashMetres(dashed, 0) === null, 'no pattern or no scale gives nothing');

if (bad) process.exit(1);
console.log('dash ok');
