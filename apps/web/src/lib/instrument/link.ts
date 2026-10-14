// The part between "bytes arrived" and "a reading exists", shared by every transport (Web Bluetooth and
// the simulator) so the simulator exercises the same decoding, duplicate handling and acknowledging
// as a real instrument: decode, acknowledge, drop repeats, report.

import { LegFilter, ackFor, decodeLeg, hex } from './caveble.ts';
import type { LinkEvents } from './types.ts';

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
