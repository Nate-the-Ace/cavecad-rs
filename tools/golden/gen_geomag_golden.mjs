// Runs the OLD CaveCAD IGRF code (CsGeomag.js) over a grid of places and dates and writes what it
// answered, so crates/cave-geomag can be held to it. Re-run only to refresh the answer key:
//   node tools/golden/gen_geomag_golden.mjs <old cave-survey dir> <out file>
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';

const [, , oldRoot = `${process.env.HOME}/Documents/github/cavecad-main/cave-survey`, outFile = 'crates/cave-geomag/tests/golden.json'] = process.argv;
const ctx = vm.createContext({ Math });
for (const f of ['CsIgrfCoeffs', 'CsGeomag']) {
  const src = fs.readFileSync(path.join(oldRoot, `scripts/CaveSurvey/Core/${f}.js`), 'utf8').replace(/^\s*include\(.*\);\s*$/gm, '');
  vm.runInContext(src.replace(/^var (\w+) =/gm, 'globalThis.$1 ='), ctx);
}
const cases = [];
for (let lat = -80; lat <= 80; lat += 20) {
  for (let lon = -170; lon <= 170; lon += 34) {
    for (const year of [1900.0, 1933.37, 1962.5, 1987.9, 2004.2, 2019.99, 2025.0, 2027.6]) {
      const r = vm.runInContext(`CsGeomag.declination(${lat}, ${lon}, ${year}, 0)`, ctx);
      cases.push([lat, lon, year, r.declination, r.inclinationMag, r.horizontalNt, r.totalNt]);
    }
  }
}
fs.mkdirSync(path.dirname(outFile), { recursive: true });
fs.writeFileSync(outFile, JSON.stringify(cases));
console.log(`${cases.length} cases -> ${outFile}`);
