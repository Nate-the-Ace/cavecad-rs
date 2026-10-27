// Runs the OLD CaveCAD .th2 reader (CsTherion2.js) over sketch files and writes what it read, so
// cave-formats' th2 port can be held to it. Re-run only to refresh the answer key:
//   node tools/golden/gen_th2_golden.mjs <out.json> <file.th2>...
// The repository's key covers the old app's own test sketch; a key made from real sketches stays
// on the machine that has them (they belong to the clubs that drew them).
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';

const oldRoot = process.env.OLD_ROOT ?? `${process.env.HOME}/Documents/github/cavecad-main/cave-survey`;
const [, , outFile, ...files] = process.argv;
const ctx = vm.createContext({ Math, JSON, RegExp, Object, Array, String, Number, isFinite, parseFloat, parseInt, isNaN });
for (const f of ['Format/CsTherion', 'Format/CsTherion2']) {
  const src = fs.readFileSync(path.join(oldRoot, `scripts/CaveSurvey/Core/${f}.js`), 'utf8').replace(/^\s*include\(.*\);\s*$/gm, '');
  vm.runInContext(src.replace(/^var (\w+) =/gm, 'globalThis.$1 ='), ctx);
}
const cases = [];
for (const file of files) {
  ctx.__text = fs.readFileSync(file, 'utf8');
  cases.push({ file: path.basename(file), input: ctx.__text, sketch: JSON.parse(vm.runInContext('JSON.stringify(CsTherion2.parse(__text))', ctx)) });
}
fs.mkdirSync(path.dirname(outFile), { recursive: true });
fs.writeFileSync(outFile, JSON.stringify(cases));
console.log(`${cases.length} sketches -> ${outFile}`);
