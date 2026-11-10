// Writes what the OLD app's title block maths gives (wrapping, title lines, name de-duplication, dates, crew):
//   node tools/golden/gen_title_golden.mjs <out.json> [Core dir]
import { readFileSync, writeFileSync } from 'node:fs';
import vm from 'node:vm';
const dir = process.argv[3] ?? `${process.env.HOME}/Documents/github/cavecad-main/cave-survey/scripts/CaveSurvey/Core`;
const ctx = vm.createContext({ isNull: (v) => v === null || v === undefined });
for (const f of ['CsSheet.js', 'CsSheetSetup.js']) vm.runInContext(readFileSync(`${dir}/${f}`, 'utf8'), ctx);
vm.runInContext('this.S = CsSheetSetup;', ctx);
const S = ctx.S;
const out = { wrap: [], lines: [], dedupe: [], dates: [], crew: [] };
for (const t of ['', 'one', 'a bb ccc dddd eeeee', 'Nathan Schonegg, Rhonda Matteson, Adam Stanich, Jeanna Smith', 'supercalifragilisticexpialidocious is long']) for (const b of [8, 12, 40]) out.wrap.push({ t, b, r: S.wrapText(t, b) });
const sets = [
  {},
  { caveName: 'Truitt Cave' },
  { caveName: 'Truitt Cave', location: 'Smith County, Tennessee', surveyedBy: 'A B, C D, E F, G H, I J, K L, M N, O P, Q R, S T, U V, W X, Y Z, Nathan Schonegg, Rhonda Matteson', date: '2024-04-06 to 2024-11-03', length: '12,345 FT', depth: '210 FT', personnel: '', copyright: '2026 CaveCAD', legendNote: 'USGS symbols', surveyMethod: 'DistoX', sheetNumber: 'A1' },
  { cartographyBy: 'N. Schonegg 2026', surveyCode: 'UISv2 5-c' },
];
for (const v of sets) out.lines.push({ v, r: S.titleLines(v, undefined) });
const names = [
  ['Nathan Schonegg', 'NATHAN SCHONEGG', 'Nathan'],
  ['Adam Stanich', 'Adam', 'Adam Smith'],
  ['Jeanna', 'Jeanne', 'Jeanna Lee'],
  ['Nathan S.', 'Nathan Schonegg', 'Rhonda', 'Rhonda Matteson'],
  ['N. Schonegg', 'Nathan Schonegg'],
  ['  Bo   Peep ', 'bo peep', ''],
  [],
];
for (const n of names) out.dedupe.push({ n, r: S.dedupeNames(n) });
for (const d of [[], [''], ['2024-04-06'], ['2024-11-03', '2024-04-06', '2024-04-06', '2023-01-01']]) out.dates.push({ d, r: S.datesFor({ trips: d.map((x) => ({ date: x })) }) });
for (const t of [[], ['A B, C D'], ['A B, C D', 'a b, Cee Dee', 'C'], [' , ,x']]) out.crew.push({ t, r: S.surveyedByFor({ trips: t.map((x) => ({ team: x })) }) });
writeFileSync(process.argv[2], JSON.stringify(out));
console.log('ok');
