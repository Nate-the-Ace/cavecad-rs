// The part between "bytes arrived" and "a reading exists", shared by every transport (Web Bluetooth and
// the simulator) so the simulator exercises the same decoding, duplicate handling and acknowledging
// as a real instrument: decode, acknowledge, drop repeats, report.

import { LegFilter, ackFor, decodeLeg, hex } from './caveble.ts';
import { X1_SHORT_PACKET_BYTES, ackFrame, decodePacket, describeErrors } from './cavway.ts';
import type { LinkEvents, Reading } from './types.ts';

/** Rounds away the noise of a 32-bit float (8.84 arrives as 8.840000152...): 0.1 mm, and a thousandth of a degree. */
const tidy = (v: number, digits: number) => Math.round(v * 10 ** digits) / 10 ** digits;

export class LegLink {
  private readonly filter = new LegFilter();
  private readonly events: LinkEvents;
  private readonly send: (byte: number) => void;
  private readonly source: string;
  /** `send` writes one command byte to the instrument. */
  constructor(events: LinkEvents, send: (byte: number) => void, source: string) {
    this.events = events;
    this.send = send;
    this.source = source;
  }

  /** A frame arrived on the leg characteristic. */
  onFrame(bytes: Uint8Array, from = 'leg') {
    this.events.frame(hex(bytes), from);
    const leg = decodeLeg(bytes);
    if (!leg) {
      this.events.frame('(not a CaveBLE leg: ignored)', from);
      return;
    }
    this.send(ackFor(leg.seq)); // always: an unacknowledged frame is sent again forever
    if (!this.filter.isNew(bytes)) return;
    this.events.reading({ distance: tidy(leg.distance, 4), azimuth: tidy(leg.azimuth, 3), inclination: tidy(leg.inclination, 3), roll: tidy(leg.roll, 3), source: this.source });
  }

  reset() {
    this.filter.reset();
  }
}

// ---- the X1 ----

/** How many recent packets are remembered to recognise one the X1 sent again. */
const RECENT = 8;

/**
 * The X1's path from bytes to readings: decode, acknowledge (every packet, a repeat too, or the X1 keeps
 * offering it), drop a packet that is a byte-for-byte repeat of a recent one (the X1 resends until it hears
 * the ack; every real shot differs, if only by its timestamp), leave calibration shots out, report the rest.
 * Shots are NOT grouped into legs here: that is `legs.ts`.
 */
export class X1Link {
  private readonly recent: string[] = [];
  private readonly events: LinkEvents;
  private readonly send: (bytes: Uint8Array) => void;
  private readonly source: string;
  /** `send` writes one already-framed message to the X1's write characteristic. */
  constructor(events: LinkEvents, send: (bytes: Uint8Array) => void, source: string) {
    this.events = events;
    this.send = send;
    this.source = source;
  }

  /** A notification arrived on the X1's read characteristic. */
  onFrame(bytes: Uint8Array, from = 'x1') {
    this.events.frame(hex(bytes), from);
    const p = decodePacket(bytes);
    if (!p) {
      this.events.frame(bytes.length < X1_SHORT_PACKET_BYTES ? '(too short for a shot packet: ignored)' : '(not a shot packet: ignored)', from);
      return;
    }
    this.send(ackFrame(p.flag));
    const key = hex(bytes);
    if (this.recent.includes(key)) return;
    this.recent.push(key);
    if (this.recent.length > RECENT) this.recent.shift();
    if (p.calibration) {
      this.events.frame('(calibration shot: acknowledged, not surveyed)', from);
      return;
    }
    const reading: Reading = {
      distance: tidy(p.distance, 4),
      azimuth: tidy(p.azimuth, 3),
      inclination: tidy(p.inclination, 3),
      roll: tidy(p.roll, 3),
      source: this.source,
      leg: p.leg,
      mark: p.mark,
      error: describeErrors(p.errors),
      quality: p.quality,
    };
    this.events.reading(reading);
  }

  reset() {
    this.recent.length = 0;
  }
}
