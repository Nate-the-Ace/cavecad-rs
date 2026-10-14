// Writes what the web app's own sketch geometry (apps/web/src/lib/sketch-geometry.ts) gives for a spread of
// surveys, for the Rust copy (crates/cave-sketch) to match:
//   node --experimental-strip-types tools/golden/gen_sketch_golden.mjs <out.json>
// Run it against the commit that still has sketch-geometry.ts (survey-sketch before the port).
import { writeFileSync } from 'node:fs';
import { traverse } from '../../apps/web/src/lib/instrument/simulator.ts';
import { anchorStroke, layout, placeStroke, planView, profileView, sectionView, shotVector } from '../../apps/web/src/lib/sketch-geometry.ts';

let n = 0;
const S = (from, to, distance, az, inc, azBack = null, incBack = null) => ({ id: `s${++n}`, from, to, distance, az, inc, azBack, incBack });

/** A survey from the simulator's steps: legs with their backsights, splays at the station they follow. */
function fromSteps(steps, prefix = 'S') {
  const shots = [];
  const dims = {};
  let k = 1;
  let leg = null;
  for (const s of steps) {
    if (s.kind === 'leg') {
      leg = S(`${prefix}${k}`, `${prefix}${k + 1}`, s.distance, s.azimuth, s.inclination);
      shots.push(leg);
      k += 1;
      dims[`${prefix}${k}`] = { left: s.walls.left, right: s.walls.right, up: s.walls.up, down: s.walls.down };
    } else if (s.kind === 'back') {
      leg.azBack = s.azimuth;
      leg.incBack = s.inclination;
    } else if (s.kind === 'splay') shots.push(S(`${prefix}${k}`, null, s.distance, s.azimuth, s.inclination));
  }
  return { shots, dims };
}

const surveys = {};
surveys.simple = fromSteps(traverse({ seed: 5, legs: 6, backsights: true, splays: 2 }));
surveys.nobacks = fromSteps(traverse({ seed: 8, legs: 9, backsights: false, splays: 4 }));
surveys.long = fromSteps(traverse({ seed: 12, legs: 25, backsights: true, splays: 4 }));
surveys.partial = (() => {
  const s = fromSteps(traverse({ seed: 5, legs: 6, backsights: true, splays: 4 }));
  // Dimensions missing at some stations and sides: runs of walls break and the passage splits.
  delete s.dims.S3;
  s.dims.S5 = { left: 1.2, right: null, up: 2, down: null };
  s.dims.S6 = { left: null, right: null, up: null, down: null };
  return s;
})();
// A branch: the main line A to F, with a side passage from C and a short one from B.
surveys.branch = {
  shots: [S('A1', 'A2', 5, 90, 0), S('A2', 'A3', 6, 100, 5), S('A3', 'A4', 4, 80, -3), S('A4', 'A5', 7, 90, 2), S('A3', 'B1', 9, 200, 10), S('B1', 'B2', 9, 210, 0), S('A2', 'C1', 2, 0, 0), S('A3', null, 2.2, 10, 40), S('B1', null, 1, 12, -50)],
  dims: { A1: { left: 1, right: 1, up: 2, down: 1 }, A2: { left: 1.1, right: 0.9, up: 2.1, down: 1 }, A3: { left: 1.5, right: 1.2, up: 2.4, down: 0.8 }, B1: { left: 0.5, right: 0.6, up: 1, down: 0.5 } },
};
// A loop, shot round: A1 to A4 and back to A1, which does not close exactly.
surveys.loop = {
  shots: [S('L1', 'L2', 10, 90, 0), S('L2', 'L3', 10, 180, 1), S('L3', 'L4', 10, 270, -1), S('L4', 'L1', 10.4, 2, 0.5, 182, -0.5)],
  dims: { L1: { left: 1, right: 1, up: 1, down: 1 }, L3: { left: 2, right: 2, up: 2, down: 2 } },
};
// A survey whose later shots are written backwards (the far end is the known station) and out of order.
surveys.backwards = { shots: [S('Z3', 'Z2', 5, 270, 1), S('Z2', 'Z1', 5, 270, 0), S('Z4', 'Z3', 3, 0, 0)], dims: {} };
// A pit: a pitch has no bearing (the compass is null) and must still be placed straight down; a shot with no
// compass that is not a pitch cannot be placed at all.
surveys.pit = { shots: [S('P1', 'P2', 4, 45, 0), S('P2', 'P3', 12, null, -90), S('P3', 'P4', 8, null, -89), S('P4', 'P5', 3, null, 10), S('P5', 'P6', 3, 90, 0), S('P3', null, 2, null, 88)], dims: { P1: { left: 1, right: 1, up: 1, down: 1 }, P3: { left: 3, right: 3, up: 0, down: 0 } } };
surveys.broken = { shots: [S('Q1', 'Q2', 0, 10, 0), S('Q2', 'Q3', 5, 10, null), S('Q3', 'Q4', 5, 10, 0), S('Q9', 'Q10', 5, 10, 0), S('Q1', null, 3, 10, 0), S('Q9', null, 3, 10, 0)], dims: {} };
surveys.splaysonly = { shots: [S('R1', null, 3, 10, 0), S('R1', null, 3, 100, 0)], dims: { R1: { left: 1, right: 2, up: 3, down: 4 } } };
surveys.empty = { shots: [], dims: {} };
surveys.wraparound = { shots: [S('W1', 'W2', 5, 359, 0, 181.5, 0), S('W2', 'W3', 5, 1, 0, 180.5, 0), S('W3', 'W4', 5, 0.5, 2, null, -2)], dims: { W2: { left: 1, right: 1, up: 1, down: 1 } } };

const out = { cases: [], anchors: [] };
for (const [name, { shots, dims }] of Object.entries(surveys)) {
  const l = layout(shots);
  const labels = Object.keys(l.stations);
  out.cases.push({
    name, shots, dims,
    vectors: shots.map((s) => shotVector(s)),
    layout: { stations: Object.values(l.stations), legs: l.legs, splays: l.splays, chain: l.chain },
    plan: planView(l, dims),
    profile: profileView(l, dims),
    sections: Object.fromEntries([...labels, 'nowhere'].map((label) => [label, sectionView(l, dims, label)])),
  });
}

// Strokes hung on stations.
const stations = [{ label: 'A', p: [0, 0] }, { label: 'B', p: [10, 0] }, { label: 'C', p: [10, 10] }];
for (const [pts, widths] of [[[[1, 1], [2, 1], [3, 2]], null], [[[9, 1], [10, 2], [11, 3], [12, 3]], [1, 1.2, 1.1, 0.9]], [[[10, 9], [9, 11]], null], [[[100, 100]], null]]) {
  const a = anchorStroke(stations, pts, 'wall', widths);
  out.anchors.push({ stations, pts, widths, anchored: a, placed: a ? placeStroke(stations, a) : null, moved: a ? placeStroke(stations.map((s) => ({ ...s, p: [s.p[0] + 5, s.p[1] - 2] })), a) : null, gone: a ? placeStroke(stations.filter((s) => s.label !== a.anchor), a) : null });
}
out.anchors.push({ stations: [], pts: [[1, 1]], widths: null, anchored: anchorStroke([], [[1, 1]], 'wall'), placed: null, moved: null, gone: null });
out.anchors.push({ stations, pts: [], widths: null, anchored: anchorStroke(stations, [], 'wall'), placed: null, moved: null, gone: null });

writeFileSync(process.argv[2], JSON.stringify(out));
console.log('sketch golden:', out.cases.length, 'surveys,', out.anchors.length, 'strokes');
