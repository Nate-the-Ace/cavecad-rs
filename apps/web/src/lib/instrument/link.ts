// The part between "bytes arrived" and "a reading exists", shared by every transport (Web Bluetooth and
// the simulator) so the simulator exercises the same decoding, duplicate handling and acknowledging
// as a real instrument: decode, acknowledge, drop repeats, report.

import { LegFilter, ackFor, decodeLeg, hex } from './caveble.ts';
import { X1_SHORT_PACKET_BYTES, ackFrame, decodePacket, describeErrors } from './cavway.ts';
import { decodeErrors, decodeMeta, decodePrimary, describeBricErrors, type BricErrorPair, type BricMeta, type BricPrimary } from './bric.ts';
import { decodeDistoXPacket } from './sap5.ts';
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

// ---- the SAP5 ----

/**
 * The SAP5's path from bytes to readings: each notification is one 8-byte DistoX packet. There is nothing to
 * acknowledge. A packet byte for byte the same as one of the last few is the same shot repeated (the sequence bit
 * flips with every real packet, so two real shots never match).
 */
export class Sap5Link {
  private readonly recent: string[] = [];
  private readonly events: LinkEvents;
  private readonly source: string;
  constructor(events: LinkEvents, source = 'sap5') {
    this.events = events;
    this.source = source;
  }

  onFrame(bytes: Uint8Array, from = 'sap5') {
    this.events.frame(hex(bytes), from);
    const p = decodeDistoXPacket(bytes);
    if (!p) {
      this.events.frame('(not a DistoX data packet: ignored)', from);
      return;
    }
    const key = hex(bytes);
    if (this.recent.includes(key)) return;
    this.recent.push(key);
    if (this.recent.length > 4) this.recent.shift();
    this.events.reading({ distance: tidy(p.distance, 4), azimuth: tidy(p.azimuth, 3), inclination: tidy(p.inclination, 3), roll: tidy(p.roll, 3), source: this.source, mark: 'none' });
  }

  reset() {
    this.recent.length = 0;
  }
}

// ---- the BRIC ----

export interface Timers {
  schedule(fn: () => void, ms: number): unknown;
  cancel(handle: unknown): void;
}
const realTimers: Timers = { schedule: (fn, ms) => setTimeout(fn, ms), cancel: (h) => clearTimeout(h as ReturnType<typeof setTimeout>) };

/** How long after a primary indication a shot is given up on waiting for its meta and error indications. */
export const BRIC_SETTLE_MS = 400;

/**
 * The BRIC's path from indications to readings. A shot is three indications (primary, then meta, then errors): it is
 * complete when the errors arrive. If they never do (some firmware sends the primary alone) it is written anyway
 * after `BRIC_SETTLE_MS`, or as soon as the next shot's primary shows it has been left behind. A primary the same as
 * the last one is a repeat. A shot with no length is not a shot. A jump in the running index is reported (shots were missed).
 */
export class BricLink {
  private lastPrimary: string | null = null;
  private lastIndex = -1;
  private pending: { primary: BricPrimary; meta: BricMeta | null; errors: [BricErrorPair, BricErrorPair] | null } | null = null;
  private timer: unknown = null;
  private readonly events: LinkEvents;
  private readonly source: string;
  private readonly timers: Timers;
  constructor(events: LinkEvents, source = 'bric', timers: Timers = realTimers) {
    this.events = events;
    this.source = source;
    this.timers = timers;
  }

  /** An indication arrived: `kind` says which characteristic. */
  onFrame(kind: 'primary' | 'meta' | 'errors', bytes: Uint8Array) {
    this.events.frame(hex(bytes), kind);
    if (kind === 'primary') {
      const primary = decodePrimary(bytes);
      if (!primary) return this.events.frame('(not a BRIC primary indication: ignored)', kind);
      const key = hex(bytes);
      if (key === this.lastPrimary) return;
      this.lastPrimary = key;
      this.flush(); // a shot still waiting for its errors has been left behind: write it as it is
      this.pending = { primary, meta: null, errors: null };
      this.timer = this.timers.schedule(() => this.flush(), BRIC_SETTLE_MS);
    } else if (kind === 'meta') {
      const meta = decodeMeta(bytes);
      if (meta && this.pending) this.pending.meta = meta;
    } else {
      const errors = decodeErrors(bytes);
      if (this.pending) {
        this.pending.errors = errors;
        this.flush();
      }
    }
  }

  /** Writes the shot being assembled, with whatever of it has arrived. */
  flush() {
    const p = this.pending;
    this.pending = null;
    if (this.timer !== null) this.timers.cancel(this.timer);
    this.timer = null;
    if (!p) return;
    const { primary, meta, errors } = p;
    if (meta) {
      if (meta.type === 0 && this.lastIndex >= 0 && meta.index > this.lastIndex + 1) this.events.frame(`(missed ${meta.index - this.lastIndex - 1} shot(s) before number ${meta.index})`, 'primary');
      this.lastIndex = meta.index;
    }
    if (primary.distance <= 0.01) {
      this.events.frame('(a shot with no length: ignored)', 'primary');
      return;
    }
    this.events.reading({
      distance: tidy(primary.distance, 4),
      azimuth: tidy(primary.azimuth, 3),
      inclination: tidy(primary.inclination, 3),
      roll: meta ? tidy(meta.roll, 3) : null,
      source: this.source,
      mark: 'none',
      error: describeBricErrors(errors),
    });
  }

  reset() {
    this.flush();
    this.lastPrimary = null;
    this.lastIndex = -1;
  }
}
