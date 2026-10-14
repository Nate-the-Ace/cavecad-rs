import { DEFAULT_SCALE, PX_PER_INCH, SCALES, describeScale, gridFor, gridLines, matchScale, stepText } from '../src/lib/sketch-scale.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const near = (a: number, b: number, e = 1e-9) => Math.abs(a - b) <= e;
const FT = 0.3048;

ok(DEFAULT_SCALE.label === `1" = 20'` && SCALES[0] === DEFAULT_SCALE, `1" = 20' is first and the default`);
// 1 inch of paper (96 px) is 20 ft of ground: 20 ft = 6.096 m.
ok(near(DEFAULT_SCALE.pxPerM, PX_PER_INCH / (20 * FT)), `at 1" = 20' an inch on paper is 20 ft of ground`);
ok(near(DEFAULT_SCALE.pxPerM * 20 * FT, 96), 'so 20 ft is 96 pixels');
ok(near(DEFAULT_SCALE.minorM, 2 * FT) && near(DEFAULT_SCALE.majorM, 20 * FT), 'the fine grid is a tenth of an inch (2 ft), the heavy line an inch (20 ft)');
ok(near(DEFAULT_SCALE.minorM * DEFAULT_SCALE.pxPerM, 9.6), 'a fine square is 9.6 pixels');

const labels = SCALES.map((s) => s.label);
ok(['1" = 5\'', '1" = 10\'', '1" = 20\'', '1" = 30\'', '1" = 40\'', '1" = 50\'', '1" = 100\'', '1 : 100', '1 : 200', '1 : 500', '1 : 1000'].every((l) => labels.includes(l)), 'the common scales are all offered');
ok(new Set(SCALES.map((s) => s.id)).size === SCALES.length, 'each has its own id');
ok(SCALES.filter((s) => s.id.startsWith('ft')).every((s) => near(s.minorM * 10, s.majorM)), 'in feet, ten fine squares to the heavy line');
const five = SCALES.find((s) => s.label === `1" = 5'`)!;
ok(near(five.pxPerM / DEFAULT_SCALE.pxPerM, 4), 'a smaller number of feet is a larger drawing: 1" = 5\' is four times 1" = 20\'');
const r500 = SCALES.find((s) => s.label === '1 : 500')!;
ok(near(r500.pxPerM, 96 / 0.0254 / 500) && near(r500.minorM, 1) && near(r500.majorM, 5), 'metric: 1 : 500 has a 2 mm fine grid (1 m) and a 10 mm heavy one (5 m)');

ok(matchScale(DEFAULT_SCALE.pxPerM * 1.001)?.id === 'ft20' && matchScale(DEFAULT_SCALE.pxPerM * 1.02) === null, 'a scale within half a percent is the preset; a zoomed one is custom');
ok(describeScale(DEFAULT_SCALE.pxPerM) === `1" = 20'`, 'a preset describes itself');
ok(describeScale(DEFAULT_SCALE.pxPerM * 20 / 17) === `1" = 17'`, `a zoomed view says what it is: 1" = 17'`);
ok(describeScale(DEFAULT_SCALE.pxPerM * 20 / 2.5) === `1" = 2.5'`, 'and a close one keeps its decimal');
ok(describeScale(0.1) === '1 : 37795' && describeScale(1) === "1\" = 315'", 'a view zoomed far out is given in big feet, then as a ratio');

const g = gridFor(DEFAULT_SCALE.pxPerM);
ok(near(g.minorM, 2 * FT) && near(g.majorM, 20 * FT), 'grid at a preset is the preset\'s');
const custom = gridFor(7);
ok(custom.minorM * 7 >= 7.5 && custom.minorM * 7 < 7.5 * 2.6, 'a zoomed view gets the smallest round fine spacing that is not too dense');
ok(near(custom.majorM / custom.minorM, [5, 10].find((k) => near(custom.majorM / custom.minorM, k)) ?? -1), 'with a heavy line every fifth or tenth');
ok(gridFor(0.0001).minorM <= 1000, 'a far-out view still has a grid spacing');
const ft = gridFor(4 / FT, 7.5, true);
ok(near(ft.minorM / FT, 2) && near(ft.majorM / FT, 10), 'zoomed to 4 px a foot, in feet: a 2 ft grid with a heavy line every 10 ft (not 3.28 ft)');
const ft2 = gridFor(30 / FT, 7.5, true);
ok(near(ft2.minorM / FT, 0.5) && near(ft2.majorM / FT, 5), 'zoomed in to 30 px a foot: a 6 in grid, heavy every 5 ft');

const lines = gridLines(-1, 5, 2, 10);
ok(lines.map((l) => l.at).join() === '0,2,4' && lines[0].heavy && !lines[1].heavy, 'lines at multiples of the step, the heavy one where it meets the major');
ok(gridLines(-25, 25, 5, 20).filter((l) => l.heavy).map((l) => l.at).join() === '-20,0,20', 'heavy lines fall on multiples of the major, either side of zero');
ok(gridLines(0, 10, 0, 10).length === 0 && gridLines(5, 5, 1, 5).length === 0, 'a zero step or an empty range has no lines');
ok(gridLines(0, 1_000_000, 1, 10).length === 0, 'an absurd number of lines is not drawn');
ok(near(gridLines(0.1, 0.5, 0.1, 0.5)[0].at, 0.1) && gridLines(0.1, 0.5, 0.1, 0.5).length === 5, 'no line is lost to float error at the ends');

ok(stepText(2 * FT, true) === '2 ft' && stepText(0.5 * FT, true) === '6 in' && stepText(20 * FT, true) === '20 ft', 'steps read in feet and inches');
ok(stepText(1, false) === '1 m' && stepText(0.2, false) === '20 cm', 'or in metres and centimetres');

if (bad) process.exit(1);
console.log('sketchscale ok');
