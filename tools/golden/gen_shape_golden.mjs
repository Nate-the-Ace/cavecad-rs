// Runs the OLD app's shaped-line geometry on a few spines and writes its answers for the Rust port to
// match: node tools/golden/gen_shape_golden.mjs <old repo root> <out.json>
import { readFileSync, writeFileSync } from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';

const [root, out] = process.argv.slice(2);
const ctx = { isNull: (x) => x == null };
vm.createContext(ctx);
for (const f of ['CsLayers.js', 'CsArea.js', 'CsShapeLine.js']) {
  vm.runInContext(readFileSync(path.join(root, 'scripts/CaveSurvey/Core', f), 'utf8'), ctx);
}
vm.runInContext('this.CsShapeLine = CsShapeLine; this.CsArea = CsArea;', ctx);
const { CsShapeLine: S, CsArea: A } = ctx;

const spines = {
  straight: { pts: [[0, 0], [30, 0]], closed: false },
  wavy: { pts: [[0, 0], [10, 6], [22, -4], [35, 3], [50, 0]], closed: false },
  loop: { pts: [[0, 0], [20, 0], [24, 14], [8, 22], [-4, 10]], closed: true },
  fin: { pts: [[0, 0], [60, 3], [120, -2]], closed: false },
};
const survey = [[5, 8], [20, 9], [40, 7], [58, 10], [30, -9], [100, 6], [70, -3]];
const o = (pts) => pts.map(([x, y]) => ({ x, y }));
const flat = (ps) => ps.map((p) => [p.x, p.y]);
const cases = [];
for (const [name, sp] of Object.entries(spines)) {
  const pts = o(sp.pts);
  const c = { name, pts: sp.pts, closed: sp.closed };
  c.stations = S.stations(pts, sp.closed, 4).map((s) => [s.x, s.y, s.tx, s.ty]);
  c.ticks = S.ticks(pts, sp.closed, 4, 2, 1).map((l) => flat(l));
  c.fans = S.fans(pts, sp.closed, 6, 3, -1, 22).map((l) => flat(l));
  const ch = S.scallops(pts, sp.closed, 3, 0.5, 1);
  c.chain = { points: flat(ch.points), bulges: ch.bulges, closed: ch.closed };
  c.survey = survey;
  c.sides = S.autoSides(pts, sp.closed, 8, o(survey), 1.2).map((e) => [e.side, e.skip, e.sure]);
  c.glyphs = [];
  for (const seed of [7, 424242]) {
    const sides = S.autoSides(pts, sp.closed, 8, o(survey), 1.2);
    const g = S.glyphPlacementsAuto(pts, sp.closed, 8, 1.2, sides, 0.4, (20 * Math.PI) / 180, 0.25, A.rng(seed));
    c.glyphs.push({ seed, placed: g.map((p) => [p.x, p.y, p.angle, p.scaleMul]) });
  }
  c.inward = sp.closed ? S.inwardSide(pts) : null;
  cases.push(c);
}
writeFileSync(out, JSON.stringify(cases));
console.log(cases.length, 'spines');
