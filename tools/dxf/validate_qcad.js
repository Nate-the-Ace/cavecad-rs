// Opens a DXF written by cave-dxf in QCAD (headless) and prints what it found.
//   /Applications/QCAD.app/Contents/MacOS/QCAD -no-gui -allow-multiple-instances \
//       -autostart tools/dxf/validate_qcad.js file.dxf
// Prints "### QCAD OK" with entity counts, or "### QCAD FAIL".
var args = RSettings.getOriginalArguments();
var path = args[args.length - 1];

var doc = new RDocument(new RMemoryStorage(), new RSpatialIndexNavel());
var di = new RDocumentInterface(doc);
var res = di.importFile(path);

var counts = {};
var ids = doc.queryAllEntities(false, true);
for (var i = 0; i < ids.length; i++) {
    var e = doc.queryEntity(ids[i]);
    var t = String(e.getType());
    counts[t] = (counts[t] || 0) + 1;
}
var layers = doc.queryAllLayers();
var names = [];
for (var j = 0; j < layers.length; j++) {
    names.push(doc.queryLayer(layers[j]).getName());
}
print("### QCAD import result: " + res);
print("### QCAD entities total: " + ids.length);
print("### QCAD by type: " + JSON.stringify(counts));
print("### QCAD layers: " + names.sort().join(", "));
print(ids.length > 0 ? "### QCAD OK" : "### QCAD FAIL");
QCoreApplication.exit(ids.length > 0 ? 0 : 1);
