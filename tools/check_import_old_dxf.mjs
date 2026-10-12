// Checks the one-off old-drawing importer against the old app's own reader: for each synthetic test
// cave, the shots and trips it reads from the old-format DXF must equal what the old reader gave
// (saved beside the DXF), and the committed notebook fixtures must be what the importer writes now.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';

const root = path.resolve(import.meta.dirname, '..');
const old = path.join(root, 'crates/cave-dxf/tests/old_app');
const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'importer-'));
const oldRoot = `${process.env.HOME}/Documents/github/cavecad-main/cave-survey`;
if (!fs.existsSync(oldRoot)) {
  console.log('importer check skipped: the old app is not on this machine');
  process.exit(0);
}
let bad = 0;
for (const name of ['PitfallCave', 'FingerprintCave', 'PlumblinePit', 'StairstepCave']) {
  const out = path.join(tmp, `${name}.json`);
  execFileSync('node', [path.join(root, 'tools/import_old_dxf.mjs'), path.join(old, `${name}.dxf`), out], { stdio: 'pipe' });
  const got = JSON.parse(fs.readFileSync(out, 'utf8'));
  const want = JSON.parse(fs.readFileSync(path.join(old, `${name}.survey.json`), 'utf8'));
  for (const key of ['shots', 'trips', 'anchorZ']) {
    if (JSON.stringify(got[key]) !== JSON.stringify(want[key])) { bad++; console.log(`FAIL ${name}: ${key} differs from the old reader`); }
  }
  const committed = path.join(root, 'crates/cave-notebook/tests/data', `${name}.json`);
  if (fs.existsSync(committed) && fs.readFileSync(committed, 'utf8') !== fs.readFileSync(out, 'utf8')) { bad++; console.log(`FAIL ${name}: committed fixture is stale; re-run tools/import_old_dxf.mjs`); }
}
fs.rmSync(tmp, { recursive: true });
if (bad) process.exit(1);
console.log('importer ok');
