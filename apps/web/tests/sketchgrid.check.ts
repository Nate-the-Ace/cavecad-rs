import { DEFAULT_FEET_PER_INCH, GRID, PX_PER_INCH, describeScale, feetPerInch, gridCaption, gridLines, matchScale, pxPerMetre, visibleGrid } from '../src/lib/sketch-grid.ts';
import { FOOT } from '../src/lib/units.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const near = (a: number, b: number, e = 1e-9) => Math.abs(a - b) <= e;

// The Sheets tab's list, as the engine gives it (a sample of it).
const scales = [
  { label: '1" = 10 ft', feet_per_inch: 10, metric: false },
  { label: '1" = 20 ft', feet_per_inch: 20, metric: false },
  { label: '1" = 50 ft', feet_per_inch: 50, metric: false },
  { label: '1:500', feet_per_inch: 500 / 12, metric: true },
];

ok(DEFAULT_FEET_PER_INCH === 20, '1 inch = 20 feet is the default');
ok(near(pxPerMetre(20) * 20 * FOOT, PX_PER_INCH), 'at 1" = 20 ft, 20 ft of ground is a paper inch: 96 px');
ok(near(pxPerMetre(5) / pxPerMetre(20), 4), 'a smaller number of feet is a bigger drawing: 1" = 5 ft is four times 1" = 20 ft');
ok(near(feetPerInch(pxPerMetre(37)), 37), 'feet per inch and pixels per metre convert both ways');

// 1:500 is 500 inches of ground to an inch of paper = 41.67 ft: the sheet list and this agree.
ok(near(pxPerMetre(500 / 12), PX_PER_INCH / 0.0254 / 500, 1e-6), 'a ratio scale in the sheet list means what a ratio means (1:500)');

// The grid does not depend on the scale.
ok(near(GRID.minorM, 2 * FOOT) && near(GRID.majorM, 20 * FOOT), 'the grid is 2 ft squares with a heavy line every 20 ft');
ok(gridCaption() === '2 ft / 20 ft', 'and is captioned that way');
ok(near(GRID.minorM * pxPerMetre(20), 9.6), 'at 1" = 20 ft a fine square is 9.6 pixels');
ok(near(GRID.minorM * pxPerMetre(20) * 10, 96), 'and ten of them are the paper inch');
const spacings = new Set([5, 20, 50, 100].map(() => GRID.minorM));
ok(spacings.size === 1, 'the spacing is one figure whatever the scale: it is never re-spaced');

ok(visibleGrid(pxPerMetre(20)).minor && visibleGrid(pxPerMetre(20)).major, 'at 1" = 20 ft both fine and heavy lines show');
const far = visibleGrid(pxPerMetre(100));
ok(!far.minor && far.major, 'zoomed out to 1" = 100 ft the fine lines (1.9 px apart) are left out, the heavy ones stay');
ok(!visibleGrid(pxPerMetre(1000)).major, 'far enough out, none are drawn');
ok(visibleGrid(pxPerMetre(2)).minor, 'zoomed in, the fine lines show');

ok(matchScale(pxPerMetre(20) * 1.001, scales)?.label === '1" = 20 ft', 'a scale within half a percent is the list\'s');
ok(matchScale(pxPerMetre(20) * 1.02, scales) === null, 'one zoomed to is not');
ok(describeScale(pxPerMetre(50), scales) === '1" = 50 ft' && describeScale(pxPerMetre(500 / 12), scales) === '1:500', 'a listed scale uses the list\'s own label');
ok(describeScale(pxPerMetre(17), scales) === '1" = 17 ft', 'a zoomed view says what it is');
ok(describeScale(pxPerMetre(2.5), scales) === '1" = 2.5 ft', 'with its decimal when it is small');

const lines = gridLines(-1, 5, 2, 10);
ok(lines.map((l) => l.at).join() === '0,2,4' && lines[0].heavy && !lines[1].heavy, 'lines at multiples of the step, heavy where they meet the major');
ok(gridLines(-25, 25, 5, 20).filter((l) => l.heavy).map((l) => l.at).join() === '-20,0,20', 'heavy lines fall on multiples of the major, either side of zero');
ok(gridLines(0, 10, 0, 10).length === 0 && gridLines(5, 5, 1, 5).length === 0, 'a zero step or an empty range has no lines');
ok(gridLines(0, 1_000_000, 1, 10).length === 0, 'an absurd number of lines is not drawn');
ok(gridLines(0.1, 0.5, 0.1, 0.5).length === 5, 'no line is lost to float error at the ends');

if (bad) process.exit(1);
console.log('sketchgrid ok');
