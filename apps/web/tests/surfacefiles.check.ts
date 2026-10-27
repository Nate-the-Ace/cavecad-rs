import { requestKey, looksLikeJpeg, looksLikeTiff, fetchWithTimeout } from '../src/lib/surface-files.ts';
const ok = (c: boolean, m: string) => { if (!c) { console.error('FAIL', m); process.exit(1); } console.log('ok', m); };
const plan = { zone: 16, bounds: { xmin: 1, ymin: 2, xmax: 3, ymax: 4 }, for_at: { lat: 0, lon: 0 }, photo: { w: 10, h: 10, url: 'u' } };
ok(requestKey(plan, 'photo') === requestKey(structuredClone(plan), 'photo'), 'key stable');
ok(requestKey(plan, 'photo') !== requestKey({ ...plan, zone: 17 }, 'photo'), 'key changes with zone');
ok(requestKey(plan, 'photo') !== requestKey({ ...plan, bounds: { ...plan.bounds, xmax: 3.5 } }, 'photo'), 'key changes with bounds');
ok(requestKey(plan, 'photo') !== requestKey({ ...plan, photo: { w: 11, h: 10, url: 'u' } }, 'photo'), 'key changes with size');
ok(requestKey(plan, 'photo') !== requestKey(plan, 'elevation'), 'key differs per part');
const json = new TextEncoder().encode('{"error":{"code":400}}');
ok(!looksLikeJpeg(json) && !looksLikeTiff(json), 'JSON error body refused');
ok(looksLikeJpeg(new Uint8Array([0xff, 0xd8, 0xff, 0xe0])), 'JPEG magic');
ok(looksLikeTiff(new Uint8Array([0x49, 0x49, 42, 0, 8])) && looksLikeTiff(new Uint8Array([0x4d, 0x4d, 0, 42, 8])), 'TIFF magic both orders');

// fetchWithTimeout aborts a hanging request
const realFetch = globalThis.fetch;
globalThis.fetch = ((_u: string, init: RequestInit) => new Promise((_res, rej) => init.signal!.addEventListener('abort', () => rej(Object.assign(new Error('aborted'), { name: 'AbortError' }))))) as typeof fetch;
const t0 = Date.now();
let msg = '';
try { await fetchWithTimeout('http://x.invalid/', 50); } catch (e) { msg = (e as Error).message; }
ok(/did not finish/.test(msg) && Date.now() - t0 < 2000, 'fetchWithTimeout aborts after the deadline');
globalThis.fetch = realFetch;

// store never throws and keeps the bytes for the session even where OPFS is missing or refuses the write.
const { store, load } = await import('../src/lib/surface-files.ts');
const setNavigator = (v: unknown) => Object.defineProperty(globalThis, 'navigator', { value: v, configurable: true, writable: true });
const realNavigator = globalThis.navigator;
setNavigator({}); // no navigator.storage at all
const a = new Uint8Array([1, 2, 3]);
ok((await store('photo-none', a)) === false, 'no OPFS: store says it was not kept on disk');
ok((await load('photo-none'))?.join() === '1,2,3', 'no OPFS: the part is still there for the session');
ok((await load('photo-never')) === null, 'an unknown key is null');
// OPFS present but the write fails (older Safari has no createWritable)
const handle = { getFile: async () => { throw new Error('none'); } };
setNavigator({ storage: { getDirectory: async () => ({ getDirectoryHandle: async () => ({ getFileHandle: async () => handle }) }) } });
let threw = false;
let kept = true;
try { kept = await store('elevation-nocw', new Uint8Array([9])); } catch { threw = true; }
ok(!threw && kept === false, 'a failing OPFS write does not throw');
ok((await load('elevation-nocw'))?.join() === '9', 'a failing OPFS write still keeps the part for the session');
// OPFS works: written, and read back
const files = new Map<string, Uint8Array>();
setNavigator({ storage: { getDirectory: async () => ({ getDirectoryHandle: async () => ({ getFileHandle: async (k: string, o?: { create?: boolean }) => {
  if (!files.has(k) && !o?.create) throw new Error('missing');
  return { createWritable: async () => ({ write: async (b: Uint8Array) => void files.set(k, b), close: async () => {} }), getFile: async () => new Blob([files.get(k)! as BlobPart]) };
} }) }) } });
ok((await store('photo-opfs', new Uint8Array([7]))) === true && files.get('photo-opfs')?.join() === '7', 'OPFS keeps it when it can');
files.set('photo-earlier', new Uint8Array([5, 5]));
ok((await load('photo-earlier'))?.join() === '5,5', 'a part kept in an earlier session comes back from OPFS');
setNavigator(realNavigator);
