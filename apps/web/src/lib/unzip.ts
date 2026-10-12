// Reads a zip: the files in it by name. Handles files stored as they are and files deflated (what
// other zip tools make), so a CaveCAD file or project that was opened and re-zipped still opens.

export interface Unzipped {
  name: string;
  data: Uint8Array;
}

async function inflate(data: Uint8Array): Promise<Uint8Array> {
  const stream = new Blob([data as BlobPart]).stream().pipeThrough(new DecompressionStream('deflate-raw'));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

/** The files in a zip, or `null` when it is not one (or is damaged). Folders are left out. */
export async function unzip(bytes: Uint8Array): Promise<Unzipped[] | null> {
  const v = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  // the end record sits in the last 64 KB + 22 bytes
  let end = -1;
  for (let i = bytes.length - 22; i >= Math.max(0, bytes.length - 22 - 65535); i--) {
    if (v.getUint32(i, true) === 0x06054b50) {
      end = i;
      break;
    }
  }
  if (end < 0) return null;
  const count = v.getUint16(end + 10, true);
  let at = v.getUint32(end + 16, true);
  const dec = new TextDecoder();
  const out: Unzipped[] = [];
  try {
    for (let n = 0; n < count; n++) {
      if (v.getUint32(at, true) !== 0x02014b50) return null;
      const method = v.getUint16(at + 10, true);
      const size = v.getUint32(at + 20, true);
      const nameLen = v.getUint16(at + 28, true);
      const extraLen = v.getUint16(at + 30, true);
      const commentLen = v.getUint16(at + 32, true);
      const local = v.getUint32(at + 42, true);
      const name = dec.decode(bytes.subarray(at + 46, at + 46 + nameLen));
      at += 46 + nameLen + extraLen + commentLen;
      if (name.endsWith('/')) continue;
      if (v.getUint32(local, true) !== 0x04034b50) return null;
      const start = local + 30 + v.getUint16(local + 26, true) + v.getUint16(local + 28, true);
      const raw = bytes.subarray(start, start + size);
      if (method === 0) out.push({ name, data: raw.slice() });
      else if (method === 8) out.push({ name, data: await inflate(raw) });
      else return null;
    }
  } catch {
    return null;
  }
  return out;
}
