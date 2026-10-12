// Runs the OLD app's area-fill scatter on a few outlines and writes its answers for the Rust port to
// match: node tools/golden/gen_fill_golden.mjs <old repo root> <out.json>
import { readFileSync, writeFileSync } from 'node:fs';
import vm from 'node:vm';
import path from 'node:path';

const [root, out] = process.argv.slice(2);
const ctx = { isNull: (x) => x == null, CsArea: undefined };
vm.createContext(ctx);
vm.runInContext(readFileSync(path.join(root, 'scripts/CaveSurvey/Core/CsArea.js'), 'utf8'), ctx);
const { CsArea } = ctx;

const outlines = {
  square: [[0, 0], [40, 0], [40, 40], [0, 40]],
  lshape: [[0, 0], [80, 0], [80, 25], [30, 25], [30, 70], [0, 70]],
  thin: [[0, 0], [120, 0], [125, 10], [0, 8]],
};
const cases = [];
for (const [name, pts] of Object.entries(outlines)) {
  const verts = pts.map(([x, y]) => ({ x, y }));
  for (const key of ['BLOCKS', 'DEBRIS', 'PEBBLES', 'SAND', 'GUANO']) {
    for (const seed of [42, 123456789]) {
      const entry = CsArea.CATALOG[key];
      const placed = CsArea.placements(verts, entry, seed, 1.0, 1.0);
      cases.push({ name, key, seed, pts, count: placed.length, first: placed.slice(0, 150).map((p) => [p.x, p.y, p.block, p.scale, p.angle]) });
    }
  }
}
writeFileSync(out, JSON.stringify(cases));
console.log(cases.length, 'cases,', cases.reduce((n, c) => n + c.count, 0), 'placements');
