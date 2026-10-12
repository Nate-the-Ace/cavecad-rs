// Writes what the OLD app's callout geometry gives for a few notes, for the Rust copy to match:
//   node tools/golden/gen_callout_golden.mjs <out.json>
import { readFileSync, writeFileSync } from 'node:fs';
import vm from 'node:vm';
const src = readFileSync('/home/user/CaveCAD/scripts/CaveSurvey/Core/CsCallout.js', 'utf8');
const ctx = vm.createContext({});
vm.runInContext(src + '\nthis.CsCallout = CsCallout;', ctx);
const C = ctx.CsCallout;
const boxes = [{ x1: 10, y1: 5, x2: 30, y2: 8 }, { x1: -4, y1: -2, x2: 0, y2: 1 }];
const tipSets = [[{ x: 0, y: 0 }], [{ x: 50, y: 10 }], [{ x: 0, y: 0 }, { x: 40, y: 20 }], [{ x: 20, y: 6 }], []];
const cases = [];
for (const box of boxes) for (const tips of tipSets) for (const side of ['auto', 'left', 'right']) for (const leader of ['straight', 'curved']) {
  const r = C.reflow(box, tips, { side, leader });
  cases.push({ box, tips, side, curved: leader === 'curved', out: { side: r.side, landing: [r.landing.x, r.landing.y], branches: r.branches.map((b) => b.map((p) => [p.x, p.y])) } });
}
writeFileSync(process.argv[2], JSON.stringify(cases));
console.log(cases.length, 'cases');
