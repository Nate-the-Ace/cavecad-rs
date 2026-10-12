// A minimal zip writer (files stored, not compressed) so a feedback report is one file that opens
// anywhere. Nothing here touches the browser: it turns names and bytes into the bytes of a zip.

const TABLE = (() => {
  const t = new Uint32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c >>> 0;
  }
  return t;
})();

/** The CRC-32 of some bytes (what a zip records for each file). */
export function crc32(data: Uint8Array): number {
  let c = 0xffffffff;
  for (const b of data) c = TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

export interface ZipEntry {
  name: string;
  data: Uint8Array;
}

/** The bytes of a zip holding these files (names are written as UTF-8). `when` is the files' time. */
export function zip(entries: ZipEntry[], when = new Date()): Uint8Array {
  const enc = new TextEncoder();
  const time = (when.getHours() << 11) | (when.getMinutes() << 5) | (when.getSeconds() >> 1);
  const date = ((Math.max(1980, when.getFullYear()) - 1980) << 9) | ((when.getMonth() + 1) << 5) | when.getDate();
  const parts: Uint8Array[] = [];
  const central: Uint8Array[] = [];
  let offset = 0;
  const u16 = (v: DataView, at: number, n: number) => v.setUint16(at, n, true);
  const u32 = (v: DataView, at: number, n: number) => v.setUint32(at, n >>> 0, true);
  for (const e of entries) {
    const name = enc.encode(e.name);
    const crc = crc32(e.data);
    const local = new Uint8Array(30 + name.length);
    const lv = new DataView(local.buffer);
    u32(lv, 0, 0x04034b50);
    u16(lv, 4, 20);
    u16(lv, 6, 0x0800); // names are UTF-8
    u16(lv, 8, 0); // stored
    u16(lv, 10, time);
    u16(lv, 12, date);
    u32(lv, 14, crc);
    u32(lv, 18, e.data.length);
    u32(lv, 22, e.data.length);
    u16(lv, 26, name.length);
    local.set(name, 30);
    const head = new Uint8Array(46 + name.length);
    const hv = new DataView(head.buffer);
    u32(hv, 0, 0x02014b50);
    u16(hv, 4, 20);
    u16(hv, 6, 20);
    u16(hv, 8, 0x0800);
    u16(hv, 10, 0);
    u16(hv, 12, time);
    u16(hv, 14, date);
    u32(hv, 16, crc);
    u32(hv, 20, e.data.length);
    u32(hv, 24, e.data.length);
    u16(hv, 28, name.length);
    u32(hv, 42, offset);
    head.set(name, 46);
    parts.push(local, e.data);
    central.push(head);
    offset += local.length + e.data.length;
  }
  const size = central.reduce((n, c) => n + c.length, 0);
  const end = new Uint8Array(22);
  const ev = new DataView(end.buffer);
  u32(ev, 0, 0x06054b50);
  u16(ev, 8, entries.length);
  u16(ev, 10, entries.length);
  u32(ev, 12, size);
  u32(ev, 16, offset);
  const all = [...parts, ...central, end];
  const out = new Uint8Array(all.reduce((n, p) => n + p.length, 0));
  let at = 0;
  for (const p of all) {
    out.set(p, at);
    at += p.length;
  }
  return out;
}
