// Has the OLD CaveCAD draw its synthetic test caves and save real old-format DXF files, then dumps
// what the old reader (CsRevise.surveyFromDocument) reads back out of each. The Rust importer must
// reproduce that survey exactly.
//
//   /Applications/CaveCAD.app/Contents/MacOS/CaveCAD -no-dock-icon -no-gui -allow-multiple-instances \
//       -autostart tools/golden/old_app_dxf.js <old cave-survey dir> <out dir>
//
// Output per cave: <out>/<Name>.dxf (the drawing) and <out>/<Name>.survey.json (the old reader's survey).
var args = RSettings.getOriginalArguments();
var oldRoot = args[args.length - 2];
var outDir = args[args.length - 1];

include("scripts/EAction.js");
include("scripts/simple.js");
includeBasePath = oldRoot + "/scripts/CaveSurvey/Core";
include(includeBasePath + "/CsAll.js");

function readText(path) {
    var f = new QFile(path);
    if (!f.open(QIODevice.ReadOnly | QIODevice.Text)) { throw new Error("cannot read " + path); }
    var s = new QTextStream(f);
    var t = s.readAll();
    f.close();
    return String(t);
}
function writeText(path, text) {
    var f = new QFile(path);
    if (!f.open(QIODevice.WriteOnly | QIODevice.Text)) { throw new Error("cannot write " + path); }
    var s = new QTextStream(f);
    s.writeString(text);
    s.flush();
    f.close();
}

var CAVES = ["PitfallCave", "FingerprintCave", "PlumblinePit", "StairstepCave"];
var failed = 0;
for (var ci = 0; ci < CAVES.length; ci++) {
    var name = CAVES[ci];
    var doc = new RDocument(new RMemoryStorage(), createSpatialIndex());
    var di = new RDocumentInterface(doc);
    getDocument = function() { return doc; };
    getDocumentInterface = function() { return di; };
    getMainWindow = function() { return null; };
    try {
        var survey = CsFormatCompass.parse(readText(oldRoot + "/testdata/" + name + ".dat"));
        var resolved = CsAdjust.resolveAndAdjust(survey, {});
        CsDraw.survey(survey, resolved);
        var dxfPath = outDir + "/" + name + ".dxf";
        var ok = di.exportFile(dxfPath, "DXF R27 (2013) Drawing", false);
        var recon = CsRevise.surveyFromDocument(doc);
        var s = recon.survey;
        var dump = {
            cave: name,
            exported: ok,
            anchorName: recon.anchorName,
            anchorZ: recon.anchorZ,
            legacy: recon.legacy,
            caveName: s.caveName, name: s.name, distanceUnit: s.distanceUnit,
            fixed: s.fixed,
            trips: (s.trips || []).map(function(t) {
                return { name: t.name, date: t.date, team: t.team, instruments: t.instruments, declination: t.declination,
                         declinationSource: t.declinationSource, distanceUnit: t.distanceUnit, startNote: t.startNote, startLrud: t.startLrud };
            }),
            shots: s.shots.map(function(x) {
                return { from: x.from, to: x.to, distance: x.distance, azimuth: x.azimuth, inclination: x.inclination,
                         backAzimuth: x.backAzimuth, backInclination: x.backInclination,
                         left: x.left, right: x.right, up: x.up, down: x.down,
                         leftOpen: !!x.leftOpen, rightOpen: !!x.rightOpen, upOpen: !!x.upOpen, downOpen: !!x.downOpen,
                         leftAll: x.leftAll, rightAll: x.rightAll, upAll: x.upAll, downAll: x.downAll,
                         splay: !!x.splay, excludeFromAll: !!x.excludeFromAll, excludeFromPlot: !!x.excludeFromPlot,
                         noAdjust: !!x.noAdjust, azimuthOmitted: !!x.azimuthOmitted, notes: x.notes, trip: x.trip, declination: x.declination };
            })
        };
        writeText(outDir + "/" + name + ".survey.json", JSON.stringify(dump));
        print("### " + name + " shots=" + dump.shots.length + " trips=" + dump.trips.length + " exported=" + ok);
    } catch (e) {
        failed++;
        print("### FAIL " + name + ": " + e);
    }
}
print(failed === 0 ? "### OLD APP DXF OK" : "### OLD APP DXF FAILED");
QCoreApplication.exit(failed === 0 ? 0 : 1);
