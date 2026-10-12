// Runs the OLD station table on the synthetic test caves and writes its rows for the Rust port to match:
//   node tools/golden/gen_stations_golden.mjs <old repo root> <out.json>
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';

const [oldRoot, out] = process.argv.slice(2);
const core = (f) => `scripts/CaveSurvey/Core/${f}.js`;
const FILES = ['CsUuid', 'CsTell', 'CsUnits', 'CsAngles', 'CsModel', 'CsTraverse', 'CsNetwork', 'CsAdjust', 'CsFrontier', 'CsStationTable', 'Format/CsCompass'].map(core);
const ctx = vm.createContext({ console, Math, JSON, Date, RegExp, Object, Array, String, Number, isFinite, parseFloat, parseInt, isNaN });
for (const f of FILES) {
  const src = fs.readFileSync(path.join(oldRoot, f), 'utf8').replace(/^\s*include\(.*\);\s*$/gm, '');
  vm.runInContext(src, ctx, { filename: f });
}
const cases = {};
for (const name of ['PitfallCave.dat', 'FingerprintCave.dat', 'PlumblinePit.dat', 'StairstepCave.dat']) {
  ctx.__text = fs.readFileSync(path.join(oldRoot, 'testdata', name), 'utf8');
  cases[name.replace('.dat', '')] = JSON.parse(vm.runInContext(`(function(){
    var s = CsFormatCompass.parse(__text);
    var r = CsNetwork.resolve(s, {});
    var rows = CsStationTable.sort(CsStationTable.rows(s, r, {}));
    return JSON.stringify({ unit: s.distanceUnit, shotNotes: s.shots.map(function(x){ return x.notes || ''; }), startNote: s.startNote || '',
      rows: rows.map(function(x){ return { station: x.station, kinds: x.kinds, trips: x.trips, degree: x.degree,
        noteText: x.noteText, z: x.z, loops: x.loops.length }; }) });
  })()`, ctx));
}
fs.writeFileSync(out, JSON.stringify(cases));
console.log(Object.entries(cases).map(([k, v]) => `${k}: ${v.rows.length} stations`).join(', '));
