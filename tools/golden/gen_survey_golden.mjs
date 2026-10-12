// Runs the OLD CaveCAD survey math (the JavaScript Core) on its synthetic test caves and writes
// what it computed, so the Rust port can be checked against it. Re-run only to refresh the answer
// key:   node tools/golden/gen_survey_golden.mjs <old cave-survey dir> <out dir>
//
// The old Core is plain JavaScript; it is loaded the same way its own unit tests load it.
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';

const [, , oldRoot = `${process.env.HOME}/Documents/github/cavecad-main/cave-survey`, outDir = 'crates/cave-survey/tests/golden'] = process.argv;
const core = (f) => `scripts/CaveSurvey/Core/${f}.js`;
const FILES = ['CsUuid', 'CsTell', 'CsUnits', 'CsAngles', 'CsModel', 'CsTraverse', 'CsNetwork', 'CsAdjust', 'Format/CsCompass'].map(core);

const ctx = vm.createContext({ console, Math, JSON, Date, RegExp, Object, Array, String, Number, isFinite, parseFloat, parseInt, isNaN });
for (const f of FILES) {
  const src = fs.readFileSync(path.join(oldRoot, f), 'utf8').replace(/^\s*include\(.*\);\s*$/gm, '');
  vm.runInContext(src, ctx, { filename: f });
}
const run = (code) => vm.runInContext(code, ctx);

const FIXTURES = ['PitfallCave.dat', 'FingerprintCave.dat', 'PlumblinePit.dat', 'StairstepCave.dat'];
fs.mkdirSync(outDir, { recursive: true });
for (const name of FIXTURES) {
  ctx.__text = fs.readFileSync(path.join(oldRoot, 'testdata', name), 'utf8');
  const out = run(`(function(){
    var s = CsFormatCompass.parse(__text);
    var r = CsNetwork.resolve(s, {});
    var a = CsAdjust.adjust(s, r, { sigmaTape: 0.1, sigmaAngle: 1.5 });
    var pick = function(st){ var o = {}; for (var k in st) { o[k] = [st[k].x, st[k].y, st[k].z]; } return o; };
    var loop = function(l){ return { from: l.from, to: l.to, error: l.error, horizontal: l.horizontal, vertical: l.vertical, traverseLength: l.traverseLength, percent: l.percent }; };
    return JSON.stringify({
      unit: s.distanceUnit,
      shots: s.shots.map(function(x){ return { from: x.from, to: x.to, distance: x.distance, azimuth: x.azimuth, inclination: x.inclination,
        backAzimuth: x.backAzimuth === undefined ? null : x.backAzimuth, backInclination: x.backInclination === undefined ? null : x.backInclination,
        azimuthOmitted: !!x.azimuthOmitted, noAdjust: !!x.noAdjust, splay: !!x.splay, excludeFromAll: !!x.excludeFromAll }; }),
      fixed: s.fixed,
      resolved: { stations: pick(r.stations), anchors: r.anchors, loops: r.loops.map(loop), ties: r.ties.map(loop), legCount: r.legs.length },
      adjusted: { converged: a.summary.converged, adjusted: a.adjusted, stations: pick(a.stations) }
    });
  })()`);
  const file = path.join(outDir, name.replace('.dat', '.json'));
  fs.writeFileSync(file, out);
  const d = JSON.parse(out);
  console.log(`${name}: ${d.shots.length} shots, ${Object.keys(d.resolved.stations).length} stations, ${d.resolved.loops.length} loops, ${d.resolved.ties.length} ties, adjusted=${d.adjusted.adjusted}`);
}
