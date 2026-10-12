// One-off importer: reads a drawing saved by the OLD (QCAD-era) CaveCAD and writes the survey inside
// it as the shot-list JSON that `cave-fixture` / the web app load. Not a file format to keep.
//
//   node tools/import_old_dxf.mjs <drawing.dxf> <out.json> [--old-root <old cave-survey dir>]
//
// The old app stored the survey as custom properties (XDATA, app "CaveSurvey", one `key=value`
// string each) on station points and leg/splay lines. The old reader, `CsRevise.surveyFromDocument`,
// is reused as it is, fed a stand-in document built from the DXF, so the answer is the one the old
// app itself would have given (checked against its own output: tests in cave-fixture).
//
// SANITISED BY CONSTRUCTION: the output has no drawing coordinates at all (not the stations', not
// the geo anchor's) and no fixed x/y; fixed points keep their elevation only. Stations are read for
// their names and elevation, never positions. Real caves' output belongs outside the repository.
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';

const args = process.argv.slice(2);
const flag = args.indexOf('--old-root');
const oldRoot =
  flag >= 0 ? args.splice(flag, 2)[1] : `${process.env.HOME}/Documents/github/cavecad-main/cave-survey`;
const [input, output] = args;
if (!input || !output) {
  console.error('usage: node tools/import_old_dxf.mjs <drawing.dxf> <out.json> [--old-root <dir>]');
  process.exit(2);
}

/** Group-code / value pairs of a text DXF (values keep inner spaces; the code line is trimmed). */
function pairs(text) {
  const lines = text.split(/\r?\n/);
  const out = [];
  for (let i = 0; i + 1 < lines.length; i += 2) out.push([lines[i].trim(), lines[i + 1]]);
  return out;
}

/** Drawing entities that carry CaveSurvey tags: their tags and (only so the stand-in works) a position. */
function readEntities(text) {
  const all = pairs(text);
  let start = all.findIndex(([c, v], i) => c === '2' && v.trim() === 'ENTITIES' && all[i - 1]?.[1].trim() === 'SECTION');
  const out = [];
  let cur = null;
  let inTags = false;
  for (let i = start + 1; i < all.length; i++) {
    const [code, value] = all[i];
    if (code === '0') {
      if (value.trim() === 'ENDSEC') break;
      cur = { type: value.trim(), tags: {}, pos: [0, 0, 0] };
      out.push(cur);
      inTags = false;
    } else if (cur) {
      if (code === '1001') inTags = value.trim() === 'CaveSurvey';
      else if (inTags && code === '1000') {
        const eq = value.indexOf('=');
        if (eq > 0) {
          const k = value.slice(0, eq);
          if (!(k in cur.tags)) cur.tags[k] = value.slice(eq + 1);
        }
      } else if (!inTags && cur.type === 'POINT') {
        if (code === '10') cur.pos[0] = parseFloat(value);
        else if (code === '20') cur.pos[1] = parseFloat(value);
        else if (code === '30') cur.pos[2] = parseFloat(value);
      }
    }
  }
  return out.filter((e) => Object.keys(e.tags).length > 0);
}

// The old Core, loaded the way tools/golden loads it.
const FILES = ['CsUuid', 'CsTell', 'CsUnits', 'CsAngles', 'CsModel', 'CsBind', 'CsTags', 'CsRevise'];
const ctx = vm.createContext({
  console, Math, JSON, Date, RegExp, Object, Array, String, Number, isFinite, parseFloat, parseInt, isNaN,
  isNull: (x) => x === null || x === undefined,
});
for (const f of FILES) {
  const file = `scripts/CaveSurvey/Core/${f}.js`;
  const src = fs.readFileSync(path.join(oldRoot, file), 'utf8').replace(/^\s*include\(.*\);\s*$/gm, '');
  vm.runInContext(src, ctx, { filename: file });
}

// A stand-in for the QCAD document: just what CsTags and CsRevise ask of it.
const entities = readEntities(fs.readFileSync(input, 'latin1'));
ctx.__entities = entities.map((e, id) => ({
  id,
  getPosition: e.type === 'POINT' ? () => ({ x: e.pos[0], y: e.pos[1], z: e.pos[2] }) : undefined,
  getCustomProperty: (group, key, dflt) => (group === 'CaveSurvey' && key in e.tags ? e.tags[key] : dflt),
}));
const dump = vm.runInContext(
  `(function(){
    var ents = __entities;
    var doc = { queryAllEntities: function(){ return ents.map(function(e){ return e.id; }); },
                queryEntity: function(id){ return ents[id]; } };
    var recon = CsRevise.surveyFromDocument(doc);
    var s = recon.survey;
    // Elevation only. A fixed point's x/y are real-world coordinates, so they are dropped.
    var fixed = {};
    for (var k in s.fixed) { fixed[k] = { z: s.fixed[k].z }; }
    return JSON.stringify({
      anchorName: recon.anchorName, anchorZ: recon.anchorZ, legacy: recon.legacy,
      caveName: s.caveName, name: s.name, unit: s.distanceUnit, fixed: fixed,
      trips: (s.trips || []).map(function(t){
        return { name: t.name, date: t.date, team: t.team, instruments: t.instruments, declination: t.declination,
                 declinationSource: t.declinationSource, distanceUnit: t.distanceUnit, startNote: t.startNote, startLrud: t.startLrud };
      }),
      shots: s.shots.map(function(x){
        return { from: x.from, to: x.to, distance: x.distance, azimuth: x.azimuth, inclination: x.inclination,
                 backAzimuth: x.backAzimuth, backInclination: x.backInclination,
                 left: x.left, right: x.right, up: x.up, down: x.down,
                 leftOpen: !!x.leftOpen, rightOpen: !!x.rightOpen, upOpen: !!x.upOpen, downOpen: !!x.downOpen,
                 leftAll: x.leftAll, rightAll: x.rightAll, upAll: x.upAll, downAll: x.downAll,
                 splay: !!x.splay, excludeFromAll: !!x.excludeFromAll, excludeFromPlot: !!x.excludeFromPlot,
                 noAdjust: !!x.noAdjust, azimuthOmitted: !!x.azimuthOmitted, notes: x.notes, trip: x.trip, declination: x.declination };
      })
    });
  })()`,
  ctx,
);
fs.writeFileSync(output, dump);
const d = JSON.parse(dump);
console.log(`${path.basename(input)}: ${d.shots.length} shots, ${d.trips.length} trips${d.legacy ? ' (LEGACY, approximate)' : ''} -> ${output}`);
