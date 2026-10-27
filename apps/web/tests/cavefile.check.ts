import { EXT, VERSION, pack, unpack } from '../src/lib/cavefile.ts';
import { unzip } from '../src/lib/unzip.ts';
import { zip } from '../src/lib/zip.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };

const cave = JSON.stringify([{ op: 'x', n: 1 }, { op: 'y', text: 'café – notes' }]);
const png = new Uint8Array([137, 80, 78, 71, 0, 1, 2, 255, 254, 0, 0, 7]);
const bytes = pack({ name: 'Truitt', app: 'cavecad-rs test', cave, scans: [{ path: '2024 Scans/Team A/page 1.png', data: png }] });
ok(bytes[0] === 0x50 && bytes[1] === 0x4b, 'a CaveCAD file is a zip');
ok(EXT === '.cavecad', 'the extension');

const back = await unpack(bytes);
ok(!!back && !('error' in back), 'it reads back');
if (back && !('error' in back)) {
  ok(back.cave === cave, 'the cave comes back exactly, accents and all');
  ok(back.manifest.name === 'Truitt' && back.manifest.version === VERSION && back.manifest.scans.length === 1, 'the manifest says what is inside');
  ok(back.scans.length === 1 && back.scans[0].path === '2024 Scans/Team A/page 1.png' && back.scans[0].data.join() === png.join(), 'a scan comes back byte for byte, under its own path');
}

const without = await unpack(pack({ name: 'a', app: 't', cave }));
ok(!!without && !('error' in without) && without.scans.length === 0, 'a file without scans is fine');

ok((await unpack(new Uint8Array([1, 2, 3, 4]))) === null, 'junk is not a CaveCAD file');
ok((await unpack(zip([{ name: 'readme.txt', data: new TextEncoder().encode('hi') }]))) === null, 'a zip that is not a cave is not a CaveCAD file');

const future = zip([
  { name: 'manifest.json', data: new TextEncoder().encode(JSON.stringify({ format: 'cavecad', version: VERSION + 1, app: 'later', name: 'x', savedAt: '', scans: [] })) },
  { name: 'cave.json', data: new TextEncoder().encode('[]') },
]);
const f = await unpack(future);
ok(!!f && 'error' in f && /newer/.test(f.error), 'a file from a newer version is refused with a reason');

// a zip made by another tool: deflated entries read too
const deflated = await (async () => {
  const raw = new TextEncoder().encode(cave.repeat(20));
  const cs = new Blob([raw]).stream().pipeThrough(new CompressionStream('deflate-raw'));
  const data = new Uint8Array(await new Response(cs).arrayBuffer());
  const stored = zip([{ name: 'a.txt', data: raw }]);
  // patch the stored zip's one entry into a deflated one: method 8, sizes, crc stay for the raw data
  const v = new DataView(stored.buffer);
  const name = 5;
  const out = new Uint8Array(stored.length - raw.length + data.length);
  out.set(stored.subarray(0, 30 + name), 0);
  out.set(data, 30 + name);
  const rest = stored.subarray(30 + name + raw.length);
  out.set(rest, 30 + name + data.length);
  const o = new DataView(out.buffer);
  o.setUint16(8, 8, true);
  o.setUint32(18, data.length, true);
  const cen = 30 + name + data.length;
  o.setUint16(cen + 10, 8, true);
  o.setUint32(cen + 20, data.length, true);
  o.setUint32(cen + 24, raw.length, true);
  const endAt = out.length - 22;
  o.setUint32(endAt + 16, cen, true);
  void v;
  return { zipped: out, raw };
})();
const got = await unzip(deflated.zipped);
ok(!!got && got.length === 1 && new TextDecoder().decode(got[0].data) === new TextDecoder().decode(deflated.raw), 'deflated entries are read');

// surface parts round trip
const sRec = '{"zone":16}', sPhoto = new Uint8Array([1, 2, 3]), sElev = new Uint8Array([4, 5]);
const withSurf = await unpack(pack({ name: 's', app: 't', cave, surface: { record: sRec, photo: sPhoto, elevation: sElev } }));
ok(!!withSurf && !('error' in withSurf) && !!withSurf.surface && withSurf.surface.record === sRec && withSurf.surface.photo?.join() === '1,2,3' && withSurf.surface.elevation?.join() === '4,5', 'surface parts come back byte for byte');
if (withSurf && !('error' in withSurf)) {
  const paths = (withSurf.manifest.surface ?? []).map((e) => `${e.path}:${e.bytes}`).sort().join();
  ok(paths === 'surface/elevation.tif:2,surface/photo.jpg:3,surface/surface.json:11', 'manifest lists the surface parts with sizes');
}
const noSurf = pack({ name: 's', app: 't', cave });
ok(!(await unzip(noSurf))!.some((f) => f.name.startsWith('surface/')), 'no surface entries without surface');
const noSurfBack = await unpack(noSurf);
ok(!!noSurfBack && !('error' in noSurfBack) && noSurfBack.surface === null, 'a file without surface unpacks with surface null');

if (bad) process.exit(1);
console.log('cavefile ok');
