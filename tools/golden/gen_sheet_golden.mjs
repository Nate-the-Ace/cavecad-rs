// Writes what the OLD app's sheet maths gives (scales, paper, fit, scale bar, furniture layout), for
// the Rust copy to match:   node tools/golden/gen_sheet_golden.mjs <out.json>
import { readFileSync, writeFileSync } from 'node:fs';
import vm from 'node:vm';
const src = readFileSync('/home/user/CaveCAD/scripts/CaveSurvey/Core/CsSheetSetup.js', 'utf8');
const ctx = vm.createContext({ isNull: (v) => v === null || v === undefined });
vm.runInContext(src + '\nthis.S = CsSheetSetup;', ctx);
const S = ctx.S;
const out = { sheets: S.SHEETS, scales: S.SCALE_ROWS, bars: [], fits: [], furniture: [], texts: [] };
for (const r of S.SCALE_ROWS) {
  out.bars.push({ fpi: r.feetPerInch, bar: S.barFor(r.feetPerInch), text: S.scaleText(r.feetPerInch) });
}
const caves = [[300, 200], [1500, 900], [4000, 700], [90, 60], [20000, 15000], [800, 3000]];
for (const sheet of S.SHEETS) for (const [w, h] of caves) for (const footer of [0, 2.5]) for (const force of [null, false, true]) {
  const f = S.fit(w, h, sheet, footer, force === null ? undefined : force);
  out.fits.push({ sheet: sheet.name, w, h, footer, force, fit: f });
}
for (const sheet of S.SHEETS) for (const turned of [false, true]) for (const th of [1, 2, 4]) {
  const widthInches = turned ? sheet.h : sheet.w;
  const f = S.furniture({ widthInches, wants: { title: true, bar: true, north: true }, titleHeight: th, reading: null });
  out.furniture.push({ sheet: sheet.name, turned, titleHeight: th, f });
}
writeFileSync(process.argv[2], JSON.stringify(out));
console.log(out.fits.length, 'fits', out.furniture.length, 'furniture');
