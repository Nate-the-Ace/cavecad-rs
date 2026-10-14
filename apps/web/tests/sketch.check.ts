import { anchorStroke, fitView, layout, placeStroke, planView, profileView, sectionView, shotVector, toScreen, toWorld, type DimsByStation, type SurveyShot } from '../src/lib/sketch-geometry.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const near = (a: number, b: number, e = 1e-6) => Math.abs(a - b) <= e;
const leg = (id: string, from: string, to: string, distance: number, az: number | null, inc: number, extra: Partial<SurveyShot> = {}): SurveyShot => ({ id, from, to, distance, az, inc, ...extra });
const splay = (id: string, from: string, distance: number, az: number | null, inc: number): SurveyShot => ({ id, from, to: null, distance, az, inc });

// Vectors.
const east = shotVector({ distance: 10, az: 90, inc: 0 })!;
ok(near(east[0], 10) && near(east[1], 0) && near(east[2], 0), 'due east, level');
const up45 = shotVector({ distance: 10, az: 0, inc: 30 })!;
ok(near(up45[1], 10 * Math.cos(Math.PI / 6)) && near(up45[2], 5), 'north and climbing 30 degrees');
const both = shotVector({ distance: 10, az: 90, inc: 10, azBack: 270.4, incBack: -10.2 })!;
ok(near(Math.atan2(both[0], both[1]) * 180 / Math.PI, 90.2, 0.01), 'a backsight is averaged in: 90 and 90.4 make 90.2');
ok(near(Math.asin(both[2] / 10) * 180 / Math.PI, 10.1, 0.01), 'and so is its clino: 10 and 10.2 make 10.1');
const north = shotVector({ distance: 5, az: 359, inc: 0, azBack: 181 })!;
ok(near(Math.atan2(north[0], north[1]) * 180 / Math.PI, 0, 0.01) && north[1] > 4.99, 'the average wraps round north');
const pitch = shotVector({ distance: 8, az: null, inc: 90 })!;
ok(near(pitch[2], 8) && near(pitch[0], 0, 1e-9) && near(pitch[1], 0, 1e-9), 'a pitch with no bearing goes straight up');
ok(shotVector({ distance: 8, az: null, inc: 30 }) === null, 'but a shot with no bearing that is not a pitch cannot be placed');
ok(shotVector({ distance: null, az: 0, inc: 0 }) === null && shotVector({ distance: 0, az: 0, inc: 0 }) === null, 'no tape, no shot');

// A chain.
const shots: SurveyShot[] = [leg('1', 'A1', 'A2', 10, 90, 0), leg('2', 'A2', 'A3', 10, 0, 0), leg('3', 'A3', 'A4', 5, 0, 90 - 0.0001 > 85 ? 20 : 0)];
const L = layout(shots);
ok(near(L.stations.A2.x, 10) && near(L.stations.A2.y, 0), 'A2 is 10 m east of A1');
ok(near(L.stations.A3.x, 10) && near(L.stations.A3.y, 10), 'A3 is 10 m north of A2');
ok(L.stations.A4.z > 1.5 && L.stations.A4.z < 2, 'A4 is up the slope');
ok(L.chain.join() === 'A1,A2,A3,A4', 'the chain runs A1 to A4');
ok(L.legs.length === 3 && L.splays.length === 0, 'three legs placed');

// Placed from the far end, and in any order.
const rev = layout([leg('2', 'A2', 'A3', 10, 0, 0), leg('1', 'A1', 'A2', 10, 90, 0)]);
ok(rev.stations.A1 && rev.stations.A3 && near(rev.stations.A3.y - rev.stations.A1.y, 10) && near(rev.stations.A3.x - rev.stations.A1.x, 10), 'a leg listed before the one it hangs on is still placed');
const reversedEnd = layout([leg('1', 'B1', 'B2', 10, 0, 0), leg('2', 'B3', 'B2', 5, 180, 0)]);
ok(near(reversedEnd.stations.B3.y, 15) , 'a leg shot toward a known station places the far end behind it');
ok(layout([]).chain.length === 0 && layout([splay('s', 'A1', 2, 0, 0)]).stations.A1 !== undefined, 'nothing, or only a splay, is fine');

// Branches: the chain follows the longer.
const br = layout([leg('1', 'A1', 'A2', 10, 90, 0), leg('2', 'A2', 'A3', 3, 0, 0), leg('3', 'A2', 'B1', 12, 90, 0)]);
ok(br.chain.join() === 'A1,A2,B1', 'at a junction the main chain takes the longer branch');
ok(br.stations.A3 !== undefined && br.legs.length === 3, 'but every leg is placed');

// A loop is drawn as it closes (no adjustment).
const loop = layout([leg('1', 'A1', 'A2', 10, 90, 0), leg('2', 'A2', 'A3', 10, 0, 0), leg('3', 'A3', 'A4', 10, 270, 0), leg('4', 'A4', 'A1', 10.5, 180, 0)]);
ok(loop.legs.length === 4, 'a closing leg is kept');

// Splays.
const withSplays = layout([leg('1', 'A1', 'A2', 10, 90, 0), splay('s1', 'A2', 2, 0, 0), splay('s2', 'Z9', 2, 0, 0)]);
ok(withSplays.splays.length === 1 && near(withSplays.splays[0].end[1], 2) && near(withSplays.splays[0].end[0], 10), 'a splay ends where it points; one from an unplaced station is dropped');

// Plan: passage walls either side of the direction of travel.
const dims: DimsByStation = { A1: { left: 1, right: 2, up: 3, down: 0.5 }, A2: { left: 1, right: 2, up: 3, down: 0.5 }, A3: { left: null, right: null, up: 2, down: 1 } };
const lineEast = layout([leg('1', 'A1', 'A2', 10, 90, 0)]);
const plan = planView(lineEast, dims);
const f2 = (v: number) => (Math.round(v * 100) / 100 + 0).toFixed(2); // no "-0.00" from float noise
const walls = plan.walls.map((w) => w.map(([x, y]) => `${f2(x)},${f2(y)}`).join(' '));
ok(walls.includes('0.00,1.00 10.00,1.00'), 'heading east, the left wall is 1 m north');
ok(walls.includes('0.00,-2.00 10.00,-2.00'), 'and the right wall is 2 m south');
ok(plan.passage.length === 1 && plan.passage[0].length === 4, 'the passage between them is one polygon');
ok(plan.legs.length === 1 && plan.stations.length === 2, 'the leg and stations are listed');
const part = planView(layout(shots), { A1: { left: 1, right: 1, up: null, down: null }, A3: { left: 1, right: 1, up: null, down: null } });
ok(part.walls.length === 0 && part.passage.length === 0, 'a wall needs two stations in a row with dimensions: lone ones draw nothing');

// Profile: the chain unrolled.
const prof = profileView(layout([leg('1', 'A1', 'A2', 10, 90, 0), leg('2', 'A2', 'A3', 10, 0, 30)]), { A1: { left: null, right: null, up: 2, down: 1 }, A2: { left: null, right: null, up: 2, down: 1 }, A3: { left: null, right: null, up: 3, down: 1 } });
ok(prof.stations.map((s) => s.p[0].toFixed(2)).join() === '0.00,10.00,18.66', 'distance along the chain is horizontal: 0, 10, then 10 + 10 cos 30');
ok(near(prof.stations[2].p[1], 5), 'elevation is height above the start');
ok(prof.walls.length === 2 && prof.passage.length === 1, 'a floor line, a ceiling line, and the passage');
ok(near(prof.walls[0][0][1], 2) && near(prof.walls[1][0][1], -1), 'ceiling 2 m above, floor 1 m below at the start');

// Section.
const sec = sectionView(lineEast, dims, 'A2')!;
ok(sec.outline !== null && sec.outline.map((p) => p.join()).join(' ') === '-1,0 0,3 2,0 0,-0.5', 'the section is left, up, right, down');
ok(sectionView(lineEast, {}, 'A2')!.outline === null, 'no dimensions, no outline');
ok(sectionView(lineEast, dims, 'nope') === null, 'an unknown station has no section');
const sp = sectionView(layout([leg('1', 'A1', 'A2', 10, 90, 0), splay('l', 'A2', 1.5, 0, 0), splay('u', 'A2', 2, 0, 90)]), dims, 'A2')!;
ok(sp.splays.length === 2 && near(sp.splays[0][0], -1.5) && near(sp.splays[0][1], 0), 'heading east, a splay north is 1.5 m to the left');
ok(near(sp.splays[1][0], 0, 1e-9) && near(sp.splays[1][1], 2), 'and one straight up is overhead');

// Strokes stay attached.
const stations = [{ label: 'A1', p: [0, 0] as [number, number] }, { label: 'A2', p: [10, 0] as [number, number] }];
const drawn: [number, number][] = [[9, 1], [10, 1.5], [11, 1]];
const a = anchorStroke(stations, drawn, 'wall')!;
ok(a.anchor === 'A2' && near(a.pts[1][0], 0) && near(a.pts[1][1], 1.5), 'a stroke hangs on the nearest station');
ok(JSON.stringify(placeStroke(stations, a)) === JSON.stringify(drawn), 'and comes back where it was drawn');
const moved = placeStroke([{ label: 'A1', p: [0, 0] }, { label: 'A2', p: [12, 3] }], a)!;
ok(near(moved[1][0], 12) && near(moved[1][1], 4.5), 'when the station moves, the stroke moves with it');
ok(placeStroke([{ label: 'A1', p: [0, 0] }], a) === null, 'a stroke whose station is gone is not placed');
ok(anchorStroke([], drawn, 'wall') === null && anchorStroke(stations, [], 'wall') === null, 'nothing to hang on, or nothing drawn');

// Fitting.
const t = fitView([[0, 0], [100, 50]], 400, 300);
const lo = toScreen(t, [0, 0]);
const hi = toScreen(t, [100, 50]);
ok(lo[0] >= 28 - 1e-6 && hi[0] <= 372 + 1e-6 && hi[1] >= 28 - 1e-6 && lo[1] <= 272 + 1e-6, 'the whole survey fits with a margin');
ok(near(t.scale, 3.44), 'true to scale: the tighter of width and height decides');
const back = toWorld(t, toScreen(t, [37, 12]));
ok(near(back[0], 37) && near(back[1], 12), 'screen and world convert both ways');
ok(fitView([[5, 5]], 400, 300).scale <= 80, 'a single point does not zoom in without limit');
ok(fitView([], 400, 300).scale > 0, 'an empty view still has a scale');

if (bad) process.exit(1);
console.log('sketch ok');
