// The BRIC4 and BRIC5 instruments' Bluetooth LE format: pure functions and constants, no Bluetooth, so every
// byte can be tested. Written from BricQt (Philip Schuchardt, MIT, github.com/vpicaver/BricQt, a client for the
// BRIC), cross-checked against TopoDroid's driver (GPL: the layout only, nothing copied); see docs/INSTRUMENTS.md.
// Not yet checked against a unit.
//
// A shot arrives as three indications, in this order: the PRIMARY (when, distance, azimuth, inclination), the
// META (its running index, dip, roll, temperature, how many samples were averaged, and whether it was a
// scan shot), and the ERRORS (up to two things the instrument did not like). Commands are plain ASCII words
// written to a control characteristic. Every number is little-endian.

const base = (short: number) => `0000${short.toString(16).padStart(4, '0')}-0000-1000-8000-00805f9b34fb`;

/** The measurement service and its characteristics. */
export const BRIC_MEASURE_SERVICE = base(0x58d0);
/** Indicates: when, distance, azimuth, inclination (20 bytes). */
export const BRIC_PRIMARY = base(0x58d1);
/** Indicates: index, dip, roll, temperature, samples, type (19 bytes). */
export const BRIC_META = base(0x58d2);
/** Indicates: two error entries (18 bytes). */
export const BRIC_ERRORS = base(0x58d3);
/** Read and write: where a download should start from (20 bytes, a time). */
export const BRIC_LAST_TIME = base(0x58d4);
/** The control service, and its characteristic that takes ASCII commands. */
export const BRIC_CONTROL_SERVICE = base(0x58e0);
export const BRIC_CONTROL = base(0x58e1);
/** The standard services it also offers. */
export const BATTERY_SERVICE = base(0x180f);
export const DEVICE_INFO_SERVICE = base(0x180a);

/** The commands, as the ASCII the control characteristic takes. */
export const BricCommand = {
  /** Fires the laser and takes a shot. */
  SHOT: 'shot',
  SCAN: 'scan',
  LASER: 'laser',
  POWER_OFF: 'power off',
  CLEAR_MEMORY: 'clear memory',
} as const;

/** A command as bytes. */
export const commandBytes = (word: string): Uint8Array => Uint8Array.from(word, (c) => c.charCodeAt(0) & 0x7f);

export const BRIC_PRIMARY_BYTES = 20;
export const BRIC_META_BYTES = 19;
export const BRIC_ERRORS_BYTES = 18;

export interface BricTime {
  year: number;
  month: number;
  day: number;
  hour: number;
  minute: number;
  second: number;
  /** Hundredths of a second. */
  centisecond: number;
}

export interface BricPrimary {
  time: BricTime;
  /** Metres. */
  distance: number;
  /** Degrees. */
  azimuth: number;
  /** Degrees, up positive. */
  inclination: number;
}

export interface BricMeta {
  /** The shot's running number in the instrument's memory: a jump means shots were missed. */
  index: number;
  dip: number;
  roll: number;
  temperature: number;
  /** How many readings were averaged for it. */
  samples: number;
  /** 0 a regular shot, 1 a scan shot. */
  type: number;
}

export interface BricErrorPair {
  code: number;
  a: number;
  b: number;
}

const view = (b: ArrayLike<number> | ArrayBufferView) => {
  const u = ArrayBuffer.isView(b) ? new Uint8Array(b.buffer, b.byteOffset, b.byteLength) : Uint8Array.from(b);
  return new DataView(u.buffer, u.byteOffset, u.byteLength);
};

const finite = (...n: number[]) => n.every(Number.isFinite);

/** The primary indication, or `null` when it is not one (too short, or not numbers). */
export function decodePrimary(data: ArrayLike<number> | ArrayBufferView): BricPrimary | null {
  const v = view(data);
  if (v.byteLength < BRIC_PRIMARY_BYTES) return null;
  const distance = v.getFloat32(8, true);
  const azimuth = v.getFloat32(12, true);
  const inclination = v.getFloat32(16, true);
  if (!finite(distance, azimuth, inclination)) return null;
  return {
    time: { year: v.getUint16(0, true), month: v.getUint8(2), day: v.getUint8(3), hour: v.getUint8(4), minute: v.getUint8(5), second: v.getUint8(6), centisecond: v.getUint8(7) },
    distance,
    azimuth,
    inclination,
  };
}

/** The meta indication, or `null`. */
export function decodeMeta(data: ArrayLike<number> | ArrayBufferView): BricMeta | null {
  const v = view(data);
  if (v.byteLength < BRIC_META_BYTES) return null;
  const dip = v.getFloat32(4, true);
  const roll = v.getFloat32(8, true);
  const temperature = v.getFloat32(12, true);
  if (!finite(dip, roll, temperature)) return null;
  return { index: v.getUint32(0, true), dip, roll, temperature, samples: v.getUint16(16, true), type: v.getUint8(18) };
}

/** The error indication: two entries of a code and two numbers. A short one (a single entry) gives just the first. */
export function decodeErrors(data: ArrayLike<number> | ArrayBufferView): [BricErrorPair, BricErrorPair] | null {
  const v = view(data);
  if (v.byteLength < 9) return null;
  const pair = (at: number): BricErrorPair => (v.byteLength >= at + 9 ? { code: v.getUint8(at), a: v.getFloat32(at + 1, true), b: v.getFloat32(at + 5, true) } : { code: 0, a: 0, b: 0 });
  return [pair(0), pair(9)];
}

/** What an error code means (the numbers are in the instrument's own units). */
const MEANINGS: Record<number, string> = {
  1: 'accelerometer 1 reading too high',
  2: 'accelerometer 2 reading too high',
  3: 'magnetometer 1 reading too high',
  4: 'magnetometer 2 reading too high',
  5: 'the two accelerometers disagree',
  6: 'the two magnetometers disagree',
  7: 'the target moved',
  8: 'laser signal too weak',
  9: 'laser signal too strong',
  10: 'laser pattern error',
  11: 'laser timed out',
  12: 'laser error',
  13: 'laser answered the wrong message',
  14: 'clino error',
  15: 'azimuth error',
};

/** One line for the log: "instrument reports: the target moved; laser signal too weak", or `null` when there is nothing to report. */
export function describeBricErrors(errors: [BricErrorPair, BricErrorPair] | null): string | null {
  if (!errors) return null;
  const said = errors.filter((e) => e.code !== 0).map((e) => MEANINGS[e.code] ?? `error ${e.code}`);
  return said.length === 0 ? null : said.join('; ');
}

/** The 20 bytes written to the LAST_TIME characteristic to say where a download should resume from. */
export function lastTimeBytes(t: BricTime): Uint8Array {
  const out = new Uint8Array(20);
  const v = new DataView(out.buffer);
  v.setUint16(0, t.year, true);
  out[2] = t.month;
  out[3] = t.day;
  out[4] = t.hour;
  out[5] = t.minute;
  out[6] = t.second;
  out[7] = t.centisecond;
  return out;
}

/** What a BRIC sends, for the tests and any simulator: the three indications of one shot. */
export function encodeShot(s: { distance: number; azimuth: number; inclination: number; index: number; roll?: number; dip?: number; type?: number; time?: Partial<BricTime>; errors?: [number, number, number][] }): { primary: Uint8Array; meta: Uint8Array; errors: Uint8Array } {
  const primary = new Uint8Array(BRIC_PRIMARY_BYTES);
  const p = new DataView(primary.buffer);
  const t = { year: 2026, month: 10, day: 14, hour: 12, minute: 0, second: 0, centisecond: 0, ...s.time };
  p.setUint16(0, t.year, true);
  primary[2] = t.month;
  primary[3] = t.day;
  primary[4] = t.hour;
  primary[5] = t.minute;
  primary[6] = t.second;
  primary[7] = t.centisecond;
  p.setFloat32(8, s.distance, true);
  p.setFloat32(12, s.azimuth, true);
  p.setFloat32(16, s.inclination, true);
  const meta = new Uint8Array(BRIC_META_BYTES);
  const m = new DataView(meta.buffer);
  m.setUint32(0, s.index, true);
  m.setFloat32(4, s.dip ?? 60, true);
  m.setFloat32(8, s.roll ?? 0, true);
  m.setFloat32(12, 20, true);
  m.setUint16(16, 8, true);
  meta[18] = s.type ?? 0;
  const errors = new Uint8Array(BRIC_ERRORS_BYTES);
  const e = new DataView(errors.buffer);
  (s.errors ?? []).slice(0, 2).forEach(([code, a, b], i) => {
    errors[i * 9] = code;
    e.setFloat32(i * 9 + 1, a, true);
    e.setFloat32(i * 9 + 5, b, true);
  });
  return { primary, meta, errors };
}
