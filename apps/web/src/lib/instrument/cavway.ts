// The Cavway X1's wire format: pure functions and constants, no Bluetooth, so every byte can be tested.
// Written from the vendor's public "Cavway X1 Communication Protocol" (v1.0), the user manual (Rev. C) and
// a reading of their Windows app's shot parser, as set out in docs/CAVWAY-X1.md; nothing here is copied
// from the GPL code. Lines marked UNVERIFIED are where two sources disagree and the real instrument decides
// (the list is at the end of docs/CAVWAY-X1.md).
//
// The X1 speaks the Nordic UART service: it pushes each stored shot on the notify characteristic, and wants
// every one acknowledged (the shot's flag byte OR 0x55) before it stops offering it. Everything written to it
// is framed: "data:", a length byte, the payload, CR LF.

/** Nordic UART service: the X1's only service. */
export const X1_SERVICE = '6e400001-b5a3-f393-e0a9-e50e24dcca9e';
/** App to X1. */
export const X1_WRITE = '6e400002-b5a3-f393-e0a9-e50e24dcca9e';
/** X1 to app (notify). */
export const X1_NOTIFY = '6e400003-b5a3-f393-e0a9-e50e24dcca9e';

/** USB: the X1 shows up as a virtual serial port with these ids (laptops only: Web Serial). */
export const X1_USB = { vendorId: 0x1a86, productId: 0x55d3 } as const;

/** Single-byte commands. */
export const X1Command = {
  QUIT_CALIBRATION: 0x30,
  ENTER_CALIBRATION: 0x31,
  TOGGLE_CALIBRATION: 0x32,
  POWER_OFF: 0x34,
  LASER_ON: 0x36,
  LASER_OFF: 0x37,
  /** Fires the laser and stores a shot, as a press of DIST does. */
  TRIGGER: 0x38,
} as const;

/** Memory addresses read with a query frame. */
export const X1Memory = {
  /** Stored shots: 64 bytes each, 0x0000 the most recent. */
  SHOTS: 0x0000,
  CLOCK: 0x8000,
  SERIAL: 0x8008,
  CALIBRATION_INFO: 0x9040,
  COEFFICIENTS: 0x9080,
  FIRMWARE: 0xe000,
  HARDWARE: 0xe004,
} as const;

/** The most a single query returns. */
export const X1_MAX_READ = 124;

/** Bytes in a full shot packet, and in the short form the BLE link may send (UNVERIFIED which: PDF says 64, the TopoDroid driver takes exactly 33). */
export const X1_PACKET_BYTES = 64;
export const X1_SHORT_PACKET_BYTES = 33;

const HEADER = [0x64, 0x61, 0x74, 0x61, 0x3a]; // "data:"
const FOOTER = [0x0d, 0x0a];

/** Wraps a payload the way the X1 expects it: `data:`, length, payload, CR LF. */
export function frame(payload: ArrayLike<number>): Uint8Array {
  const body = Array.from(payload);
  return Uint8Array.from([...HEADER, body.length, ...body, ...FOOTER]);
}

/** The frame for a single-byte command. */
export const command = (cmd: number): Uint8Array => frame([cmd]);

/** The acknowledgement for a shot packet: its flag byte OR 0x55, framed (UNVERIFIED: both sources say so, the unit confirms). */
export const ackFrame = (flag: number): Uint8Array => frame([(flag | 0x55) & 0xff]);

/** A memory query: `3d`, the address (low, high), how many bytes (at most 124). */
export function queryFrame(address: number, length: number): Uint8Array {
  if (!Number.isInteger(address) || address < 0 || address > 0xffff) throw new RangeError('address out of range');
  if (!Number.isInteger(length) || length < 1 || length > X1_MAX_READ) throw new RangeError('length out of range');
  return frame([0x3d, address & 0xff, address >> 8, length]);
}

/** What a memory query comes back as: `3d` (or `3e` for a write echo), the address, a length, then the bytes. */
export function parseReply(bytes: ArrayLike<number>): { command: number; address: number; data: Uint8Array } | null {
  const b = Array.from(bytes);
  if (b.length < 4 || (b[0] !== 0x3d && b[0] !== 0x3e)) return null;
  const length = b[3];
  if (b.length < 4 + length) return null;
  return { command: b[0], address: b[1] | (b[2] << 8), data: Uint8Array.from(b.slice(4, 4 + length)) };
}

/** The mark a surveyor sets on a shot with the X1's M button. */
export type X1Mark = 'none' | 'feature' | 'ridge' | 'backsight' | 'generic';
const MARKS: X1Mark[] = ['feature', 'ridge', 'backsight', 'generic'];

/** One accuracy complaint from the X1 (its two sensor pairs disagree). */
export interface X1AccuracyError {
  kind: 'absG' | 'absM' | 'dip' | 'angle';
  /** The two sensors' figures (absG, absM and dip: one each); angle has just `a`. Raw units. */
  a: number;
  b: number | null;
}

/** A shot or calibration packet, decoded. Fields beyond a short packet's length are `null`. */
export interface X1Packet {
  /** `1` shot, `2` calibration. */
  type: number;
  /** The flag byte as sent: needed for the acknowledgement. */
  flag: number;
  /** The device marked it as part of a leg (three or more close shots). Bit 7 of the flag, clear = leg. */
  leg: boolean;
  calibration: boolean;
  mark: X1Mark;
  /** Metres. */
  distance: number;
  /** Degrees, 0 to 360, as measured (magnetic). */
  azimuth: number;
  /** Degrees, up positive. */
  inclination: number;
  roll: number;
  /** Seconds from the device's own clock (local wall-clock, not UTC). */
  time: number | null;
  /** Sensor 1 and 2 raw figures (gravity and magnetic magnitude, dip degrees). */
  sensors: { absG: number; absM: number; dip: number }[];
  errors: X1AccuracyError[];
  /** Laser signal quality, raw. */
  quality: number | null;
}

const ANGLE = 360 / 0xffff;
const u16 = (b: Uint8Array, i: number) => b[i] | (b[i + 1] << 8);
const i16 = (b: Uint8Array, i: number) => (u16(b, i) << 16) >> 16;

/** The accuracy block (bytes 45 on). A first byte of 0xFF means none. Bits 7, 6, 5, 4 set to 0 mean absG, absM, dip, angle error, in that order; each takes four bytes after the first, and two at most fit. */
export function parseErrors(b: Uint8Array): X1AccuracyError[] {
  if (b.length === 0 || b[0] === 0xff) return [];
  const out: X1AccuracyError[] = [];
  let at = 1;
  const kinds: [number, X1AccuracyError['kind']][] = [[7, 'absG'], [6, 'absM'], [5, 'dip'], [4, 'angle']];
  for (const [bit, kind] of kinds) {
    if (((b[0] >> bit) & 1) !== 0) continue;
    if (at + (kind === 'angle' ? 1 : 3) >= b.length) break;
    const first = u16(b, at);
    const second = kind === 'angle' ? null : u16(b, at + 2);
    // absG and absM are in ten-thousandths; a dip error is a signed angle, an angle error an unsigned one.
    const scaled = (v: number) => (kind === 'absG' || kind === 'absM' ? v / 10000 : (kind === 'dip' ? (v << 16) >> 16 : v) * ANGLE);
    out.push({ kind, a: scaled(first), b: second === null ? null : scaled(second) });
    at += 4;
    if (out.length === 2) break;
  }
  return out;
}

/** One line for the log: "accuracy error: absG, dip", or `null` for none. */
export function describeErrors(errors: X1AccuracyError[]): string | null {
  return errors.length === 0 ? null : `accuracy error: ${errors.map((e) => e.kind).join(', ')}`;
}

/**
 * Decodes a shot or calibration packet. Accepts the short form (33 bytes or more) as well as the full 64,
 * and returns `null` for anything else: too short, an unknown type, or an empty slot (first byte 0xFF).
 */
export function decodePacket(data: ArrayLike<number> | ArrayBufferView): X1Packet | null {
  const b = ArrayBuffer.isView(data) ? new Uint8Array(data.buffer, data.byteOffset, data.byteLength) : Uint8Array.from(data);
  if (b.length < X1_SHORT_PACKET_BYTES) return null;
  const type = b[0];
  if (type !== 0x01 && type !== 0x02) return null;
  const flag = b[1];
  const markBits = (flag >> 1) & 0x07;
  // The distance bytes are in an unusual order: bits 16 to 23, then 0 to 7, then 8 to 15.
  const distance = ((b[2] << 16) | (b[4] << 8) | b[3]) / 1000;
  const sensor = (at: number) => ({ absG: u16(b, at), absM: u16(b, at + 2), dip: i16(b, at + 4) * ANGLE });
  const sensors = [sensor(11)];
  if (b.length >= 64) sensors.push({ absG: u16(b, 58), absM: u16(b, 60), dip: i16(b, 62) * ANGLE });
  return {
    type,
    flag,
    leg: (flag & 0x80) === 0,
    calibration: type === 0x02,
    mark: MARKS[markBits] ?? 'none',
    distance,
    azimuth: u16(b, 5) * ANGLE,
    inclination: i16(b, 7) * ANGLE,
    roll: u16(b, 9) * ANGLE,
    time: b.length >= 21 ? (b[17] | (b[18] << 8) | (b[19] << 16) | (b[20] << 24)) >>> 0 : null,
    sensors,
    errors: b.length >= 54 ? parseErrors(b.subarray(45, 54)) : [],
    quality: b.length >= 58 ? u16(b, 56) : null,
  };
}

/** What a packet looks like on the wire, for the simulator and for tests (a full 64 bytes, or the 33-byte short form). */
export function encodePacket(p: {
  type?: number;
  leg?: boolean;
  mark?: X1Mark;
  distance: number;
  azimuth: number;
  inclination: number;
  roll?: number;
  time?: number;
  short?: boolean;
  /** Accuracy error bytes (9), default none. */
  errors?: number[];
}): Uint8Array {
  const out = new Uint8Array(X1_PACKET_BYTES).fill(0);
  const mm = Math.round(p.distance * 1000);
  const angle = (deg: number) => Math.round((((deg % 360) + 360) % 360) / ANGLE) & 0xffff;
  const markBits = Math.max(0, MARKS.indexOf(p.mark ?? 'none'));
  out[0] = p.type ?? 0x01;
  // Flag: bit 7 clear = leg; bit 0 set = not a calibration shot; bits 1 to 3 the mark (none is written as 0x07: all clear is feature).
  // The unused bits 4 to 6 are written as 1, like erased memory, so an ordinary shot is 0xff.
  out[1] = ((p.leg ? 0 : 1) << 7) | 0x70 | ((p.mark && p.mark !== 'none' ? markBits : 0x07) << 1) | ((p.type ?? 1) === 2 ? 0 : 1);
  out[2] = (mm >> 16) & 0xff;
  out[3] = mm & 0xff;
  out[4] = (mm >> 8) & 0xff;
  const put16 = (at: number, v: number) => {
    out[at] = v & 0xff;
    out[at + 1] = (v >> 8) & 0xff;
  };
  put16(5, angle(p.azimuth));
  put16(7, Math.round(p.inclination / ANGLE) & 0xffff);
  put16(9, angle(p.roll ?? 0));
  const t = p.time ?? 0;
  out[17] = t & 0xff;
  out[18] = (t >>> 8) & 0xff;
  out[19] = (t >>> 16) & 0xff;
  out[20] = (t >>> 24) & 0xff;
  const errs = p.errors ?? [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff];
  errs.forEach((v, i) => (out[45 + i] = v));
  return p.short ? out.slice(0, X1_SHORT_PACKET_BYTES) : out;
}
