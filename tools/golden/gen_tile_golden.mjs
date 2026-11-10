// Writes what the OLD app's sheet tiling (CsSheetTile) gives, for the Rust copy to match:
//   node tools/golden/gen_tile_golden.mjs <out.json> [Core dir]
import { readFileSync, writeFileSync } from 'node:fs';
import vm from 'node:vm';
const dir = process.argv[3] ?? `${process.env.HOME}/Documents/github/cavecad-main/cave-survey/scripts/CaveSurvey/Core`;
const ctx = vm.createContext({ isNull: (v) => v === null || v === undefined });
for (const f of ['CsSheetSetup.js', 'CsSheetTile.js']) vm.runInContext(readFileSync(`${dir}/${f}`, 'utf8'), ctx);
vm.runInContext('this.T = CsSheetTile; this.S = CsSheetSetup;', ctx);
const T = ctx.T;
const out = { ids: [], axes: [], layouts: [] };
for (const r of [0, 1, 25, 26, 27, 51, 52, 700]) out.ids.push({ row: r, letter: T.rowLetter(r), id: T.idOf(r, r % 7) });
for (const [c, m, o] of [[10, 20, 2], [20, 20, 2], [20.5, 20, 2], [100, 20, 2], [55, 12.5, 1.25], [1e-3, 5, 0.1]]) out.axes.push({ c, m, o, axis: T.axis(c, m, o) });
const box = (a, b, c, d) => ({ minX: a, minY: b, maxX: c, maxY: d });
const caves = [
  { cave: box(0, 0, 100, 60) },
  { cave: box(0, 0, 4000, 700) },
  { cave: box(-500, -300, 900, 1500) },
  { cave: box(0, 0, 3000, 3000), occupied: [box(0, 0, 3000, 300), box(0, 0, 300, 3000)] }, // an L
  { cave: box(0, 0, 9000, 4000), occupied: [box(0, 0, 9000, 200), box(4000, 0, 4300, 4000)] }, // a T
  { cave: box(0, 0, 1e5, 1e5) }, // too many
  { cave: box(5e5, 3.9e6, 5e5 + 2500, 3.9e6 + 1800) },
];
const papers = [[11, 8.5], [17, 11], [36, 24]];
for (const [ci, c] of caves.entries()) for (const sheet of papers) for (const scale of [20, 50, 200]) for (const turned of [false, true, undefined]) for (const footer of [0, 2.5]) for (const shift of [[0, 0], [1.5, -0.75]]) {
  const o = { caveBox: c.cave, sheet: { w: sheet[0], h: sheet[1] }, scale, turned, footerInches: footer, shiftInches: { x: shift[0], y: shift[1] } };
  if (c.occupied) o.occupied = c.occupied;
  const L = T.layout(o);
  const l = { tiled: L.tiled, tooMany: L.tooMany === true, rows: L.rows, cols: L.cols, turned: L.turned, overlap: L.overlap, anchor: L.anchor, span: L.span, matchLines: L.matchLines,
    ids: L.tiles.map((t) => t.id), big: L.tiles.length > 24,
    tiles: (L.tiles.length > 24 ? [] : L.tiles).map((t) => ({ id: t.id, row: t.row, col: t.col, title: t.title, primary: t.primary, core: t.core, map: t.map, paper: t.paper, matches: t.matches })) };
  out.layouts.push({ case: ci, sheet, scale, turned: turned ?? null, footer, shift, layout: l });
}
writeFileSync(process.argv[2], JSON.stringify(out));
console.log(out.layouts.length, 'layouts');
