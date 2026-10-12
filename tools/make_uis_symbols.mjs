#!/usr/bin/env node
// Draws the UIS (International Union of Speleology) point symbols the catalogue did not have, from
// the official list at https://www.carto.net/neumann/caving/cave-symbols/ (names, and the example
// pictures of each symbol in plan and profile). They are redrawn here as vectors, not traced:
// each is a few lines. Re-running replaces the `uis` set in crates/cave-symbols/data/symbols.json.
//
//   node tools/make_uis_symbols.mjs
//
// What is left out on purpose: the UIS entries that are line styles (outline of a gallery,
// underlying passage, presumed dimensions, ceiling form, dripline, step, contour and gradient lines,
// joints and faults) or area fills (water, sump, sinter pools, flowstone, sand, clay-covered walls),
// which belong to the line and fill tools; and the ones that are text (height of a room, difference
// in elevation, altitude figure), which need the text tool.
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const DATA = path.resolve(import.meta.dirname, '../crates/cave-symbols/data/symbols.json');

// ---- a tiny drawing vocabulary (y up; closed shapes may be filled) ----
const line = (...pts) => ({ t: 'poly', pts });
const poly = (pts, fill = false) => ({ t: 'poly', pts, closed: true, ...(fill ? { fill: true } : {}) });
const circle = (c, r, fill = false) => ({ t: 'circle', c, r, ...(fill ? { fill: true } : {}) });
const arc = (cx, cy, r, a0, a1, n = 14) => Array.from({ length: n + 1 }, (_, i) => { const a = ((a0 + ((a1 - a0) * i) / n) * Math.PI) / 180; return [cx + r * Math.cos(a), cy + r * Math.sin(a)]; });
const wave = (x, y0, y1, amp, cycles, n = 40) => Array.from({ length: n + 1 }, (_, i) => { const t = i / n; return [x + amp * Math.sin(t * cycles * 2 * Math.PI), y0 + (y1 - y0) * t]; });
const head = (x, y, size = 0.28) => poly([[x, y], [x - size * 0.6, y - size], [x + size * 0.6, y - size]], true); // filled arrowhead pointing up

const S = []; // [key, name, category, layer, prims]
const add = (key, name, category, layer, prims) => S.push({ key: 'sym-uis-' + key, name, category, layer, prims });

add('entrance-arrow', 'Entrance arrow', 'Structure', 'ENTRANCE', [poly([[0, 1], [0.3, -1], [0, -0.55], [-0.3, -1]], true)]);
add('chimney', 'Chimney', 'Structure', 'CLIMBS-CHIMNEYS', [circle([0, 0], 1), line([-0.55, 0], [0.55, 0]), line([0, -0.55], [0, 0.55])]);
add('pit-marker', 'Pit (depth mark)', 'Structure', 'PITS-DOMES', [circle([0, 0], 1), line([-0.55, 0], [0.55, 0])]);
add('chimney-pit', 'Chimney and pit', 'Structure', 'CLIMBS-CHIMNEYS', [circle([0, 0], 1), line([-0.5, 0.42], [0.5, 0.42]), line([0, 0.05], [0, 0.8]), line([-0.5, -0.45], [0.5, -0.45])]);
add('stream-flow', 'Stream flow (wavy arrow)', 'Water', 'WATER-FLOW-ARROWS', [line(...wave(0, -1, 0.7, 0.22, 2.5)), head(0, 1, 0.5)]);
add('seepage', 'Seepage / water inlet', 'Water', 'WATER-DRIP-SEEP', [-0.7, 0, 0.7].flatMap((x) => [line(...wave(x, -1, 0.55, 0.12, 1.5)), head(x, 0.9, 0.34)]));
add('ice', 'Ice, snow, firn', 'Floor', 'ICE-SNOW', [-0.8, 0, 0.8].flatMap((y) => [line([-0.9, y], [0.9, y]), line([-0.9, y - 0.2], [-0.9, y + 0.2]), line([0.9, y - 0.2], [0.9, y + 0.2]), line([0, y - 0.2], [0, y + 0.2])]));
add('cauliflower-calcite', 'Cauliflower calcite', 'Formations', 'FORMATIONS-MOONMILK-POPCORN', [line([-1, -0.9], [1, -0.9]), ...[-0.65, 0, 0.65].flatMap((x) => [line([x, -0.9], [x, 0.3]), circle([x, 0.6], 0.32, true)])]);
add('disk', 'Disc', 'Formations', 'FORMATIONS-DRIP', [circle([0, 0.55], 0.4), line([0, 0.15], [0, -0.35]), line([-0.45, -1], [0, -0.35], [0.45, -1])]);
add('stalagmite', 'Stalagmite', 'Formations', 'FORMATIONS-DRIP', [line([0, 1], [0, -0.2]), line([-0.45, -0.8], [0, -0.2], [0.45, -0.8])]);
add('stalactite', 'Stalactite', 'Formations', 'FORMATIONS-DRIP', [line([0, -1], [0, 0.2]), line([-0.45, 0.8], [0, 0.2], [0.45, 0.8])]);
add('curtain', 'Curtain / drapery', 'Formations', 'FORMATIONS-DRAPERY', [line([0, -1], [0, -0.1]), line(...arc(0.18, -0.1, 0.18, 180, 360, 8)), line([-0.45, 0.8], [0, 0.2], [0.45, 0.8]), line([0, 0.2], [0, -0.1])]);
add('pillar', 'Pillar', 'Formations', 'FORMATIONS-DRIP', [line([0, -0.6], [0, 0.6]), line([-0.45, -1], [0, -0.6], [0.45, -1]), line([-0.45, 1], [0, 0.6], [0.45, 1])]);
add('helictite', 'Helictite', 'Formations', 'FORMATIONS-MOONMILK-POPCORN', [line([0, -1], [0, 1]), line([-0.55, 0.55], [0, 0.55]), line([0, 0.1], [0.55, 0.1]), line([-0.55, -0.35], [0, -0.35]), line([0, -0.8], [0.55, -0.8])]);
add('soda-straw', 'Soda straw', 'Formations', 'FORMATIONS-DRIP', [line([-1, 0.8], [1, 0.8]), ...[-0.85, -0.45, -0.05, 0.35, 0.75].map((x, i) => line([x, 0.8], [x, 0.5 - (i % 2) * 0.8 - 0.2]))]);
add('crystal', 'Crystal', 'Formations', 'FORMATIONS-MOONMILK-POPCORN', [0, 60, 120].map((d) => { const a = (d * Math.PI) / 180; return line([-Math.cos(a), -Math.sin(a)], [Math.cos(a), Math.sin(a)]); }).flat());
add('moonmilk', 'Moonmilk / popcorn', 'Formations', 'FORMATIONS-MOONMILK-POPCORN', [poly([...arc(-0.66, -0.3, 0.33, 180, 0, 8), ...arc(0, -0.3, 0.33, 180, 0, 8), ...arc(0.66, -0.3, 0.33, 180, 0, 8)])]);
add('wall-calcite', 'Wall calcite', 'Formations', 'FORMATIONS-FLOWSTONE', [-0.65, 0, 0.65].map((x) => line([x - 0.3, -0.3], [x, 0.3], [x + 0.3, -0.3])));
add('flowstone-dashes', 'Flowstone (dashes)', 'Formations', 'FORMATIONS-FLOWSTONE', [-0.6, 0, 0.6].map((x) => line([x - 0.3, -0.7], [x + 0.3, 0.7])));
add('pebbles', 'Pebbles', 'Floor', 'SEDIMENT-SAND-GRAVEL', [poly([[-0.9, 0.1], [-0.6, 0.4], [-0.3, 0.2], [-0.4, -0.15], [-0.75, -0.2]]), poly([[0.1, 0.75], [0.45, 0.9], [0.7, 0.6], [0.5, 0.35], [0.15, 0.4]]), poly([[0.2, -0.3], [0.55, -0.1], [0.85, -0.35], [0.6, -0.7], [0.25, -0.65]])]);
add('guano', 'Guano', 'Floor', 'GUANO', [line([0, -1], [0, -0.15]), line([0, -0.15], [-0.3, 0.3], [-0.5, 0.8], [-0.85, 0.65]), line([0, -0.15], [0.3, 0.3], [0.5, 0.8], [0.85, 0.65])]);
add('bone', 'Bone', 'Archaeology', 'ARCHAEOLOGY', [poly([[-0.75, 0.14], [0.75, 0.14], [0.75, -0.14], [-0.75, -0.14]], true), ...[[-0.85, 0.25], [-0.85, -0.25], [0.85, 0.25], [0.85, -0.25]].map((c) => circle(c, 0.2, true))]);
add('anastomosis', 'Anastomosis', 'Geology', 'GEOLOGY-JOINTS-FRACTURES', [line(...Array.from({ length: 61 }, (_, i) => { const t = i / 60; return [-1 + 2 * t, Math.abs(Math.sin(t * 3 * Math.PI)) * 1.6 - 0.8 + 0.0 * t]; }))]);
add('karren', 'Karren', 'Geology', 'GEOLOGY-JOINTS-FRACTURES', [line([-1, -0.8], [-0.75, 0.8], [-0.45, -0.3], [-0.1, 0.8], [0.2, -0.3], [0.55, 0.8], [0.8, -0.8])]);
add('scallop', 'Scallop', 'Geology', 'GEOLOGY-JOINTS-FRACTURES', [poly([[0, 1], ...arc(0, -0.45, 0.5, 20, -200, 18)])]);
add('flute', 'Flute', 'Geology', 'GEOLOGY-JOINTS-FRACTURES', [line([-1, 0.2], [1, 0.2]), line(...arc(0, 0.2, 0.6, 180, 360, 14))]);
add('paleoflow', 'Paleoflow direction', 'Geology', 'GEOLOGY-JOINTS-FRACTURES', [line([0, -1], [0, 0.65]), head(0, 1, 0.45), line(...arc(0, -0.35, 0.3, 90, 270, 10))]);
add('inclined-joint', 'Inclined joint (dip)', 'Geology', 'GEOLOGY-JOINTS-FRACTURES', [poly([[-0.28, 1], [0.28, 1], [0.28, 0.1], [0.65, 0.1], [0, -1], [-0.65, 0.1], [-0.28, 0.1]])]);
add('human-activity', 'Human activity', 'Archaeology', 'ARCHAEOLOGY', [circle([0, 0], 1), ...[90, 210, 330].map((d) => { const a = (d * Math.PI) / 180; return line([0, 0], [Math.cos(a), Math.sin(a)]); })]);
add('camp', 'Camp', 'Rigging', 'ANCHORS-BOLTS', [line([-1, -0.9], [1, -0.9]), poly([[0, 0.7], [0.75, -0.9], [-0.75, -0.9]]), poly([[0, 0.25], [0.4, -0.9], [-0.4, -0.9]], true), line([0, 0.7], [0, 1]), poly([[0, 1], [0.35, 0.9], [0, 0.8]], true)]);
add('altitude', 'Altitude point', 'Survey', 'CTRL-STATIONS', [circle([0, 0], 0.28), line([-1, 0], [-0.28, 0]), line([0.28, 0], [1, 0]), line([0, -1], [0, -0.28]), line([0, 0.28], [0, 1])]);
add('survey-point', 'Survey point', 'Survey', 'CTRL-STATIONS', [poly([[0, 0.9], [0.9, -0.7], [-0.9, -0.7]])]);
add('north-arrow', 'North arrow', 'Sheet', 'NORTH-ARROW', [poly([[0, 1], [0.3, -0.85], [0, -0.45], [-0.3, -0.85]]), poly([[0, 1], [0.3, -0.85], [0, -0.45]], true)]);
add('air-draught', 'Air draught', 'Hazards', 'NOTES-ANNOTATION', [line([-0.12, -1], [-0.12, 0.35]), line([0.12, -1], [0.12, 0.35]), poly([[-0.5, 0.35], [0, 1], [0.5, 0.35]]), line([-0.4, -0.65], [0.4, -0.65])]);
add('section-arrow', 'Cross-section direction arrow', 'Survey', 'CROSS-SECTION-MARKERS', [line([0, -1], [0, 0.5], [0.55, 0.5]), poly([[1, 0.5], [0.55, 0.75], [0.55, 0.25]], true)]);

// ---- fit each to the unit circle around its drawing's middle, then merge into the catalogue ----
const k4 = (v) => Math.round(v * 1e4) / 1e4;
function unit(prims) {
  const pts = prims.flatMap((p) => (p.t === 'circle' ? [[p.c[0] - p.r, p.c[1] - p.r], [p.c[0] + p.r, p.c[1] + p.r]] : p.pts));
  const cx = (Math.min(...pts.map((p) => p[0])) + Math.max(...pts.map((p) => p[0]))) / 2, cy = (Math.min(...pts.map((p) => p[1])) + Math.max(...pts.map((p) => p[1]))) / 2;
  const far = Math.max(...prims.flatMap((p) => (p.t === 'circle' ? [Math.hypot(p.c[0] - cx, p.c[1] - cy) + p.r] : p.pts.map((q) => Math.hypot(q[0] - cx, q[1] - cy)))));
  const f = (q) => [k4((q[0] - cx) / far), k4((q[1] - cy) / far)];
  return prims.map((p) => (p.t === 'circle' ? { ...p, c: f(p.c), r: k4(p.r / far) } : { ...p, pts: p.pts.map(f) }));
}
const catalogue = JSON.parse(readFileSync(DATA, 'utf8')).filter((s) => s.set !== 'uis');
for (const s of S) catalogue.push({ key: s.key, block: '', set: 'uis', name: s.name, uis: s.name, category: s.category, layer: s.layer, prims: unit(s.prims) });
catalogue.sort((a, b) => (a.category + a.name).localeCompare(b.category + b.name));
writeFileSync(DATA, JSON.stringify(catalogue) + '\n');
console.log(`${S.length} UIS symbols; catalogue now ${catalogue.length}`);
