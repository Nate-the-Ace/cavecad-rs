// Runs the OLD CaveCAD survey-file readers and writers (Compass, Walls, Survex, CSV, Therion) and
// writes down everything they answered, so the Rust port in crates/cave-formats can be checked
// against them. Re-run only to refresh the answer key:
//   node tools/golden/gen_formats_golden.mjs <old cave-survey dir> <out file>
//
// Two sources of cases:
//   1. the old unit tests (tests/js_unit.js), run with every parse/write/detect call recorded --
//      the hand-solved edge cases the old app was written against;
//   2. every survey file in testdata/, parsed and then written back out in all five formats.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import vm from 'node:vm';

const [, , oldRoot = `${process.env.HOME}/Documents/github/cavecad-main/cave-survey`, outFile = 'crates/cave-formats/tests/golden/cases.json'] = process.argv;
const FORMATS = ['compass', 'walls', 'survex', 'csv', 'therion'];

// ---- 1. the old unit tests, recorded -------------------------------------------------------
const RECORDER = `
var __REC = [];
(function () {
  var clone = function (v) { return v === undefined ? null : JSON.parse(JSON.stringify(v)); };
  var F = { compass: CsFormatCompass, walls: CsFormatWalls, survex: CsFormatSurvex, csv: CsFormatCsv, therion: CsFormatTherion };
  Object.keys(F).forEach(function (id) {
    var o = F[id], p = o.parse, w = o.write;
    o.parse = function (c, opts) {
      var r = p.apply(o, arguments);
      if (!opts && typeof c === "string") { __REC.push({ format: id, op: "parse", input: c, output: clone(r) }); }
      return r;
    };
    o.write = function (s) {
      var before = clone(s);
      var r = w.apply(o, arguments);
      __REC.push({ format: id, op: "write", input: before, output: r });
      return r;
    };
  });
  var d = CsFormatRegistry.detect;
  CsFormatRegistry.detect = function (n, c) {
    var r = d.apply(CsFormatRegistry, arguments);
    if (typeof c === "string") { __REC.push({ op: "detect", name: n === undefined || n === null ? "" : String(n), input: c, output: r ? r.id : null }); }
    return r;
  };
  process.on("exit", function () { require("fs").writeFileSync(process.env.REC_OUT, JSON.stringify(__REC)); });
})();
`;
const unitSrc = fs.readFileSync(path.join(oldRoot, 'tests/js_unit.js'), 'utf8');
const anchor = 'for (var ci = 0; ci < CORE_FILES.length; ci++) {\n    loadRepoScript(CORE_FILES[ci]);\n}\n';
if (!unitSrc.includes(anchor)) throw new Error('js_unit.js changed shape: the CORE_FILES load loop was not found');
// The old suite no longer loads under node on its own (a Core file calls Qt's qsTr at load time);
// these stand-ins are what its QCAD engine would supply.
const SHIMS = 'globalThis.qsTr = function (t) { return t; };\nglobalThis.qsTranslate = function (c, t) { return t; };\n';
const patched = SHIMS + unitSrc
  .replace('repoRoot = nodePath.resolve(__dirname, "..");', `repoRoot = ${JSON.stringify(path.resolve(oldRoot))};`)
  .replace(anchor, anchor + RECORDER);
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'formats-golden-'));
const script = path.join(tmp, 'js_unit_recorded.js');
const recOut = path.join(tmp, 'rec.json');
fs.writeFileSync(script, patched);
try {
  execFileSync('node', [script], { env: { ...process.env, REC_OUT: recOut }, stdio: ['ignore', 'ignore', 'inherit'] });
} catch {
  // a failing old test still leaves its recorded calls behind; the answers are what the old code did
}
const recorded = JSON.parse(fs.readFileSync(recOut, 'utf8'));

// ---- 2. the test survey files --------------------------------------------------------------
const core = (f) => `scripts/CaveSurvey/Core/${f}.js`;
const FILES = ['CsUnits', 'CsAngles', 'CsModel', 'CsTraverse', 'Format/CsCompass', 'Format/CsWalls', 'Format/CsSurvex', 'Format/CsCsv', 'Format/CsTherion', 'Format/CsRegistry'].map(core);
const ctx = vm.createContext({ console, Math, JSON, Date, RegExp, Object, Array, String, Number, isFinite, parseFloat, parseInt, isNaN });
for (const f of FILES) {
  const src = fs.readFileSync(path.join(oldRoot, f), 'utf8').replace(/^\s*include\(.*\);\s*$/gm, '');
  vm.runInContext(src, ctx, { filename: f });
}
const run = (code) => vm.runInContext(code, ctx);

const files = [];
const testdata = path.join(oldRoot, 'testdata');
for (const name of fs.readdirSync(testdata).sort()) {
  if (/\.(dat|srv|svx|csv|th)$/i.test(name)) files.push(name);
}
const fileCases = [];
for (const name of files) {
  ctx.__name = name;
  ctx.__text = fs.readFileSync(path.join(testdata, name), 'utf8');
  fileCases.push(JSON.parse(run(`(function () {
    var f = CsFormatRegistry.detect(__name, __text);
    var s = f.parse(__text);
    var writes = {};
    ${JSON.stringify(FORMATS)}.forEach(function (id) {
      writes[id] = CsFormatRegistry.byId(id).write(JSON.parse(JSON.stringify(s)));
    });
    return JSON.stringify({ name: __name, input: __text, detect: f.id, survey: s, writes: writes });
  })()`)));
}

// ---- 3. a Therion project split across files (input followed) ------------------------------
const project = path.join(testdata, 'therion_project');
const projectFiles = {};
const walk = (dir) => {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) walk(p);
    else projectFiles[path.relative(testdata, p).split(path.sep).join('/')] = fs.readFileSync(p, 'utf8');
  }
};
walk(project);
ctx.__files = projectFiles;
const projectCase = JSON.parse(run(`(function () {
  var read = function (p) {
    var n = String(p).replace(/^\\.\\//, "");
    var parts = [];
    n.split("/").forEach(function (x) { if (x === "..") { parts.pop(); } else if (x !== "." && x !== "") { parts.push(x); } });
    n = parts.join("/");
    return __files.hasOwnProperty(n) ? __files[n] : null;
  };
  var s = CsFormatTherion.parse(__files["therion_project/main.th"], { path: "therion_project/main.th", readFile: read });
  return JSON.stringify({ path: "therion_project/main.th", files: __files, survey: s });
})()`));

// ---- write, dropping repeats ---------------------------------------------------------------
const seen = new Set();
const unit = recorded.filter((c) => {
  const key = JSON.stringify(c);
  if (seen.has(key)) return false;
  seen.add(key);
  return true;
});
fs.mkdirSync(path.dirname(outFile), { recursive: true });
fs.writeFileSync(outFile, JSON.stringify({ unit, files: fileCases, therionProject: projectCase }, null, 1));
const count = (op) => unit.filter((c) => c.op === op).length;
console.log(`${unit.length} recorded unit-test calls (${count('parse')} parse, ${count('write')} write, ${count('detect')} detect), ${fileCases.length} test files, therion project with ${Object.keys(projectFiles).length} files`);
console.log(`wrote ${outFile} (${(fs.statSync(outFile).size / 1024).toFixed(0)} KB)`);
