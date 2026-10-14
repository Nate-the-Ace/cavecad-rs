// The open "CaveBLE" protocol that paperless cave-surveying instruments speak to TopoDroid and
// SexyTopo (Phil Underwood, MIT; reference client `CaveBLE.kt` in github.com/furbrain/CircuitPython_CaveBLE).
// This file is only the wire format: pure functions and constants, no Bluetooth, so it can be tested
// with hand-built frames. VERIFIED against the reference client's source, NOT against a Cavway X1:
// whether the X1 speaks it is checked on the real instrument (see docs/HANDOFF-survey.md).
//
// One service holds a name (read), a command (write, no response) and a leg (notify) characteristic.
// A leg frame is 17 bytes, little-endian: u8 sequence bit, then four f32: azimuth (degrees), inclination
// (degrees), roll (degrees), distance (metres). The surveyor acknowledges every leg by writing
// `0x55 + sequence bit` to the command characteristic; an instrument that hears no ack resends the
// same frame, so a repeat of the last frame is a duplicate, not a new shot.

export const CAVE_BLE_SERVICE = '137c4435-8a64-4bcb-93f1-3792c6bdc965';
export const NAME_CHARACTERISTIC = '137c4435-8a64-4bcb-93f1-3792c6bdc966';
export const COMMAND_CHARACTERISTIC = '137c4435-8a64-4bcb-93f1-3792c6bdc967';
export const LEG_CHARACTERISTIC = '137c4435-8a64-4bcb-93f1-3792c6bdc968';

/** Single-byte commands written to the command characteristic. */
export const Command = {
  STOP_CAL: 0x30,
  START_CAL: 0x31,
  DEVICE_OFF: 0x34,
  LASER_ON: 0x36,
  LASER_OFF: 0x37,
  TAKE_SHOT: 0x38,
  ACK: 0x55,
} as const;

/** Bytes in one leg frame. */
export const LEG_BYTES = 17;

/** One shot as the instrument sent it: raw, nothing converted. */
export interface Leg {
  /** The sequence bit (0 or 1) the ack must echo. */
  seq: 0 | 1;
  /** Compass, degrees (as the instrument reports: magnetic unless it says otherwise). */
  azimuth: number;
  /** Clino, degrees, up positive. */
  inclination: number;
  /** Roll of the instrument, degrees (not used by the survey; kept for the log). */
  roll: number;
  /** Tape, metres. */
  distance: number;
}

/** Reads one leg frame, or `null` when it is not one (wrong length, unknown sequence byte, not a number). */
export function decodeLeg(data: ArrayLike<number> | ArrayBufferView): Leg | null {
  const bytes = ArrayBuffer.isView(data) ? new Uint8Array(data.buffer, data.byteOffset, data.byteLength) : Uint8Array.from(data);
  if (bytes.length !== LEG_BYTES) return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const seq = view.getUint8(0);
  if (seq !== 0 && seq !== 1) return null;
  const azimuth = view.getFloat32(1, true);
  const inclination = view.getFloat32(5, true);
  const roll = view.getFloat32(9, true);
  const distance = view.getFloat32(13, true);
  if (![azimuth, inclination, roll, distance].every(Number.isFinite)) return null;
  return { seq, azimuth, inclination, roll, distance };
}

/** The frame an instrument would send for a leg (for the simulator and for tests). */
export function encodeLeg(leg: Leg): Uint8Array {
  const out = new Uint8Array(LEG_BYTES);
  const view = new DataView(out.buffer);
  view.setUint8(0, leg.seq);
  view.setFloat32(1, leg.azimuth, true);
  view.setFloat32(5, leg.inclination, true);
  view.setFloat32(9, leg.roll, true);
  view.setFloat32(13, leg.distance, true);
  return out;
}

/** The command byte that acknowledges a leg with this sequence bit (`0x55` or `0x56`). */
export const ackFor = (seq: 0 | 1): number => Command.ACK + seq;

/** Frames as hex, for the capture log (the way a sniffed packet is written down). */
export function hex(data: ArrayLike<number> | ArrayBufferView): string {
  const bytes = ArrayBuffer.isView(data) ? new Uint8Array(data.buffer, data.byteOffset, data.byteLength) : Uint8Array.from(data);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join(' ');
}

/**
 * Tells a resend from a new shot. An instrument repeats a frame until it is acknowledged, so the same
 * sequence bit with exactly the same numbers as the last leg is the same shot arriving twice. (Two
 * real shots never agree to the last bit of four floats.) The repeat must still be acknowledged.
 */
export class LegFilter {
  private last: Uint8Array | null = null;
  /** True for a new shot, false for a repeat of the previous frame. */
  isNew(frame: Uint8Array): boolean {
    const same = this.last !== null && this.last.length === frame.length && this.last.every((b, i) => b === frame[i]);
    this.last = Uint8Array.from(frame);
    return !same;
  }
  reset() {
    this.last = null;
  }
}
