// The zip writer: reads its own output back (names, sizes, CRCs) and checks the real `unzip` accepts it.
import { execFileSync } from 'node:child_process';
import { mkdtempSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { crc32, zip } from '../src/lib/zip.ts';

let bad = 0;
const check = (ok: boolean, what: string) => {
  if (!ok) {
    bad++;
    console.error('FAIL', what);
  }
};

check(crc32(new TextEncoder().encode('123456789')) === 0xcbf43926, 'crc32 of the standard test string');
const files = [
  { name: 'feedback.txt', data: new TextEncoder().encode('Hello, caves — ünï') },
  { name: 'screenshot.png', data: Uint8Array.from({ length: 5000 }, (_, i) => (i * 7) & 255) },
  { name: 'empty.txt', data: new Uint8Array(0) },
];
const bytes = zip(files, new Date(2026, 9, 12, 8, 30, 10));
const v = new DataView(bytes.buffer);
check(v.getUint32(bytes.length - 22, true) === 0x06054b50, 'ends with the end-of-directory record');
check(v.getUint16(bytes.length - 22 + 10, true) === files.length, 'counts every file');
// walk the central directory
let at = v.getUint32(bytes.length - 22 + 16, true);
for (const f of files) {
  check(v.getUint32(at, true) === 0x02014b50, `central record for ${f.name}`);
  check(v.getUint32(at + 16, true) === crc32(f.data), `crc of ${f.name}`);
  check(v.getUint32(at + 24, true) === f.data.length, `size of ${f.name}`);
  const nameLen = v.getUint16(at + 28, true);
  check(new TextDecoder().decode(bytes.slice(at + 46, at + 46 + nameLen)) === f.name, `name of ${f.name}`);
  at += 46 + nameLen;
}
try {
  const dir = mkdtempSync(join(tmpdir(), 'zipcheck-'));
  writeFileSync(join(dir, 'a.zip'), bytes);
  const out = execFileSync('unzip', ['-t', join(dir, 'a.zip')], { encoding: 'utf8' });
  check(/No errors detected/.test(out), 'unzip -t accepts it');
} catch (e) {
  if ((e as NodeJS.ErrnoException).code !== 'ENOENT') check(false, `unzip -t: ${e}`);
  else console.log('(unzip not installed: skipped the outside check)');
}
if (bad) process.exit(1);
console.log('zip ok');
