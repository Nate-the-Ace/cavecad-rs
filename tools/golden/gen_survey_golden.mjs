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
const FILES = ['CsUuid', 'CsTell', 'CsUnits', 'CsAngles', 'CsModel', 'CsTraverse', 'CsNetwork', 'CsAdjust', 'CsLrud', 'CsProfile', 'CsClosure', 'CsFrontier', 'CsMesh3d', 'Format/CsCompass'].map(core);

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
        azimuthOmitted: !!x.azimuthOmitted, noAdjust: !!x.noAdjust, splay: !!x.splay, excludeFromAll: !!x.excludeFromAll, excludeFromPlot: !!x.excludeFromPlot,
        left: x.left, right: x.right, up: x.up, down: x.down, leftOpen: !!x.leftOpen, rightOpen: !!x.rightOpen, upOpen: !!x.upOpen, downOpen: !!x.downOpen, trip: x.trip }; }),
      name: s.caveName || s.name, startLrud: s.startLrud, fixed: s.fixed,
      trips: (s.trips || []).map(function(t){ return { name: t.name, startLrud: t.startLrud || null }; }),
      profile: (function(){
        var g = CsProfile.groupRuns(r), adj = CsProfile.adjacency(r), idx = CsProfile.legIndex(r), bands = {};
        var h = CsProfile.hierarchy(g, r);
        for (var i = 0; i < g.order.length; i++) {
          var key = g.order[i], run = g.runs[key];
          var tie = (h.ties[key] !== null && h.ties[key] !== undefined && r.stations.hasOwnProperty(h.ties[key])) ? h.ties[key] : null;
          var b = CsProfile.unrollBand(run, tie, r, null, { adjacency: adj, legIndex: idx });
          var w = CsProfile.bandWallRuns(b, s, r, {});
          var xy = function(p){ return [p.x, p.y]; };
          bands[key] = { tie: b.tie, stopped: b.stopped, stoppedReason: b.stoppedReason, omitted: b.omitted,
                         walls: { ceiling: w.ceiling.map(function(l){ return l.map(xy); }), floor: w.floor.map(function(l){ return l.map(xy); }),
                                  flat: w.flat.map(function(f){ return { at: [f.x, f.y], station: f.station }; }), skipped: w.skipped },
                         stations: b.stations.map(function(x){ return [x.name, x.x, x.y]; }), runStations: run.stations };
        }
        return { order: g.order, ungrouped: g.ungrouped, bands: bands, hierarchy: { parents: h.parents, ties: h.ties, bandOrder: h.order } };
      })(),
      mesh: (function(){
        var round = function(a, d){ var k = Math.pow(10, d); return a.map(function(v){ return Math.round(v * k) / k; }); };
        var depth = CsMesh3d.build(s, r, { colorBy: 'depth' });
        var m = CsMesh3d.build(s, r, { colorBy: 'trip' });
        return { positions: round(m.triangles.positions, 4), normals: round(m.triangles.normals, 4), colors: round(m.triangles.colors, 3),
                 depthColors: round(depth.triangles.colors, 3), indexCount: m.triangles.indices.length,
                 lineCount: m.lines.indices.length / 2, bounds: m.bounds };
      })(),
      walls: (function(){ var w = CsLrud.wallRuns(s, r, 'slope'); var run = function(x){ return { points: x.points.map(function(p){ return [p.x, p.y]; }), stations: x.stations }; }; return { left: w.left.map(run), right: w.right.map(run), skipped: w.skipped }; })(),
      resolved: { stations: pick(r.stations), anchors: r.anchors, loops: r.loops.map(loop), ties: r.ties.map(loop), legCount: r.legs.length },
      adjusted: { converged: a.summary.converged, adjusted: a.adjusted, stations: pick(a.stations) }
    });
  })()`);
  const file = path.join(outDir, name.replace('.dat', '.json'));
  fs.writeFileSync(file, out);
  const d = JSON.parse(out);
  console.log(`${name}: ${d.shots.length} shots, ${Object.keys(d.resolved.stations).length} stations, ${d.resolved.loops.length} loops, ${d.resolved.ties.length} ties, adjusted=${d.adjusted.adjusted}`);
}
