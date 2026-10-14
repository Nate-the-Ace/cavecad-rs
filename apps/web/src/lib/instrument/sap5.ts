// The Shetland Attack Pony 5's Bluetooth LE format: the classic DistoX 8-byte data packet, notified over the
// Nordic UART service. Pure functions and constants. Written from TopoDroid's SAP and DistoX handling (GPL: the
// layout only, nothing copied) and the SAP5 firmware repo (github.com/furbrain/SAP5); see docs/INSTRUMENTS.md.
// Not yet checked against a unit. (The SAP6 uses the CaveBLE service: `caveble.ts`.)
//
// Note the Nordic UART characteristics are the other way round to the Cavway X1's: the SAP5 sends on
// 6e400002 and is written to on 6e400003.

/** Nordic UART service: the SAP5's only service (the Cavway X1 uses the same one). */
export const SAP5_SERVICE = '6e400001-b5a3-f393-e0a9-e50e24dcca9e';
/** SAP5 to app (notify). */
export const SAP5_NOTIFY = '6e400002-b5a3-f393-e0a9-e50e24dcca9e';
/** App to SAP5. */
export const SAP5_WRITE = '6e400003-b5a3-f393-e0a9-e50e24dcca9e';

export const DISTOX_PACKET_BYTES = 8;

/** A data packet, decoded. */
export interface DistoXPacket {
  /** The sequence bit (bit 7 of the first byte), which flips with each packet: it tells two identical shots from a repeat. */
  seq: 0 | 1;
  /** Metres. */
  distance: number;
  /** Degrees, 0 to 360. */
  azimuth: number;
  /** Degrees, up positive. */
  inclination: number;
  /** Degrees. */
  roll: number;
}

/**
 * Decodes one 8-byte DistoX data packet, or `null` for anything else (another length, or another packet type).
 * `fixBit16` applies the SAP5 firmware's known fault: it sets the distance's bit 16 when a distance passes
 * 32.768 m, not 65.536 m (reported by its author, 2023), so a set bit with the next byte's top bit also set is
 * cleared. On by default, as in TopoDroid.
 */
export function decodeDistoXPacket(data: ArrayLike<number> | ArrayBufferView, options: { fixBit16?: boolean } = {}): DistoXPacket | null {
  const b = ArrayBuffer.isView(data) ? new Uint8Array(data.buffer, data.byteOffset, data.byteLength).slice() : Uint8Array.from(data);
  if (b.length !== DISTOX_PACKET_BYTES) return null;
  if ((b[0] & 0x3f) !== 0x01) return null;
  if ((options.fixBit16 ?? true) && (b[2] & 0x80) !== 0) b[0] &= 0xbf;
  const mm = (b[0] & 0x40) * 1024 + ((b[2] << 8) | b[1]);
  const az = (b[4] << 8) | b[3];
  const cl = ((b[6] << 8) | b[5]) << 16 >> 16;
  return {
    seq: (b[0] >> 7) & 1 ? 1 : 0,
    distance: mm / 1000,
    azimuth: (az * 180) / 32768,
    inclination: (cl * 90) / 16384,
    roll: (b[7] * 180) / 128,
  };
}

/** What a SAP5 sends, for the tests: a shot as an 8-byte packet. */
export function encodeDistoXPacket(p: { distance: number; azimuth: number; inclination: number; roll?: number; seq?: 0 | 1 }): Uint8Array {
  const mm = Math.round(p.distance * 1000);
  const out = new Uint8Array(8);
  out[0] = 0x01 | (mm >= 65536 ? 0x40 : 0) | (p.seq ? 0x80 : 0);
  out[1] = mm & 0xff;
  out[2] = (mm >> 8) & 0xff;
  const az = Math.round((((p.azimuth % 360) + 360) % 360) * 32768 / 180) & 0xffff;
  out[3] = az & 0xff;
  out[4] = az >> 8;
  const cl = Math.round(p.inclination * 16384 / 90) & 0xffff;
  out[5] = cl & 0xff;
  out[6] = cl >> 8;
  out[7] = Math.round(((p.roll ?? 0) % 360) * 128 / 180) & 0xff;
  return out;
}
