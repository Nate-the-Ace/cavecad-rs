// What every way of getting shots into the survey agrees on: a link reports a status, raw frames (for
// the capture log) and readings. The survey session only ever sees `Reading`s.

/** One shot ready for the survey: metres and degrees, as the instrument measured them. */
export interface Reading {
  distance: number;
  /** Degrees, as the instrument reports it (magnetic: the trip's declination is applied by the notebook). */
  azimuth: number;
  /** Degrees, up positive. */
  inclination: number;
  /** Instrument roll, degrees, when it reports one. */
  roll: number | null;
  /** Which link sent it, for the log. */
  source: string;
  // What an X1 adds. All optional: another instrument just leaves them out.
  /** The instrument says this shot is part of a leg (three or more close shots). */
  leg?: boolean;
  /** The mark set on the shot with the instrument's own button. */
  mark?: Mark;
  /** The instrument's accuracy complaint, if it made one (its two sensor pairs disagreed). */
  error?: string | null;
  /** Laser signal quality, raw. */
  quality?: number | null;
}

/** A mark the surveyor sets on a shot at the instrument. */
export type Mark = 'none' | 'feature' | 'ridge' | 'backsight' | 'generic';

export type LinkStatus = 'idle' | 'connecting' | 'connected' | 'lost' | 'unsupported' | 'error';

export interface LinkEvents {
  status(status: LinkStatus, detail?: string): void;
  /** Every raw frame, as hex, with where it came from (a characteristic uuid or `sim`). */
  frame(hex: string, from: string): void;
  reading(reading: Reading): void;
}

/** A way of reaching an instrument. */
export interface Instrument {
  readonly name: string;
  /** True when the instrument can be told to fire a shot from the screen. */
  readonly canTakeShot: boolean;
  /** True when it sends single shots and the device itself says what is a leg: they must be grouped (see `legs.ts`). */
  readonly groupsLegs: boolean;
  connect(): Promise<void>;
  disconnect(): void;
  takeShot(): void;
}
