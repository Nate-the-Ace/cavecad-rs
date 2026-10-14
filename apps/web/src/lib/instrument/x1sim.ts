// A pretend Cavway X1: the general simulator (`devsim.ts`) set to an X1. Kept under its own name because the first
// simulator was an X1 and its tests and options are written for it. One press of "take shot" gives ONE shot, as on the
// real unit: a leg is three close shots, a splay one, a backsight leg three shots marked backsight.

import { SimulatedInstrument, shotsFor, type FlagPolicy, type ShotSpec } from './devsim.ts';
import type { Step } from './simulator.ts';
import type { LinkEvents } from './types.ts';

export { shotsFor };
export type { FlagPolicy, ShotSpec };

export interface X1SimOptions {
  steps?: Step[];
  policy?: FlagPolicy;
  /** Each packet is delivered twice, as the X1 does when it has not heard the ack. */
  repeatFrames?: boolean;
  /** Send the 33-byte short packet instead of 64. */
  short?: boolean;
}

export class X1SimulatedInstrument extends SimulatedInstrument {
  constructor(events: LinkEvents, opts: X1SimOptions = {}) {
    super(events, { kind: 'x1', steps: opts.steps, policy: opts.policy, quirks: [...(opts.repeatFrames ? ['lost-acks'] : []), ...(opts.short ? ['short'] : [])] });
  }
}
