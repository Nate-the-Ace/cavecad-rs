// Turns the X1's single shots into the things a survey is made of. The X1 does not send "a leg": it sends
// every shot, and calls three or more consecutive close shots a leg (its screen says LEG, and the packets
// carry a leg flag). So: shots that are close to the one before join a group; a group of three (or one the
// device flags as a leg) is ONE leg, whose reading is the average of its shots; any other shot is a splay.
//
// Two things the real instrument has to settle (docs/CAVWAY-X1.md, UNVERIFIED): whether all three shots of
// a leg carry the flag or only one, and how close "close" is for the device. This code does not depend on
// the first: it decides by closeness and treats the flag as an extra vote (a flagged shot makes a leg at once,
// whatever our count). The second is `isClose`, one function with its numbers named, to be adjusted.
//
// Pure and deterministic, tested in tests/legs.check.ts with all three flag behaviours.

import type { Mark, Reading } from './types.ts';

/** Two shots of one leg agree to this much: tape 5 cm or 0.5 %, whichever is larger; compass and clino 2 degrees (a guess until the unit shows what its LEG decision uses). */
export const CLOSE_DISTANCE_M = 0.05;
export const CLOSE_DISTANCE_FRACTION = 0.005;
export const CLOSE_ANGLE_DEG = 2;
/** Beyond this the compass means nothing (a pitch): only the tape and clino are compared. */
export const STEEP_DEG = 85;
/** A leg is three close shots. */
export const SHOTS_PER_LEG = 3;

const mod360 = (a: number) => ((a % 360) + 360) % 360;
const angleGap = (a: number, b: number) => Math.abs(((((a - b) % 360) + 540) % 360) - 180);
const tidy = (v: number, digits: number) => Math.round(v * 10 ** digits) / 10 ** digits;

/** True when `b` could be the next shot of the same leg as `a`. */
export function isClose(a: Reading, b: Reading): boolean {
  if (Math.abs(a.distance - b.distance) > Math.max(CLOSE_DISTANCE_M, CLOSE_DISTANCE_FRACTION * Math.max(a.distance, b.distance))) return false;
  if (Math.abs(a.inclination - b.inclination) > CLOSE_ANGLE_DEG) return false;
  const steep = Math.abs(a.inclination) > STEEP_DEG && Math.abs(b.inclination) > STEEP_DEG;
  return steep || angleGap(a.azimuth, b.azimuth) <= CLOSE_ANGLE_DEG;
}

/** How far apart the shots of a leg were: worth a warning when large. */
export interface Spread {
  distance: number;
  azimuth: number;
  inclination: number;
}

/** What the assembler reports. */
export type LegEvent =
  /** A shot that is not part of a leg. */
  | { kind: 'splay'; reading: Reading }
  /** A leg is complete: the average of its shots. `group` names it for later updates. */
  | { kind: 'leg'; group: number; reading: Reading; shots: number; mark: Mark; spread: Spread; flagged: boolean }
  /** Another close shot joined a leg already reported: the new average, to replace the old. */
  | { kind: 'leg-update'; group: number; reading: Reading; shots: number; mark: Mark; spread: Spread; flagged: boolean };

interface Group {
  id: number;
  shots: Reading[];
  /** The leg has been reported. */
  leg: boolean;
}

/** Average of the shots of a leg: tape and clino by arithmetic mean, compass by circular mean. */
export function average(shots: Reading[]): { reading: Reading; spread: Spread } {
  const n = shots.length;
  const mean = (f: (r: Reading) => number) => shots.reduce((t, r) => t + f(r), 0) / n;
  const sin = mean((r) => Math.sin((r.azimuth * Math.PI) / 180));
  const cos = mean((r) => Math.cos((r.azimuth * Math.PI) / 180));
  const azimuth = mod360((Math.atan2(sin, cos) * 180) / Math.PI);
  const last = shots[n - 1];
  const range = (f: (r: Reading) => number) => Math.max(...shots.map(f)) - Math.min(...shots.map(f));
  const reading: Reading = {
    ...last,
    distance: tidy(mean((r) => r.distance), 4),
    azimuth: tidy(azimuth, 3),
    inclination: tidy(mean((r) => r.inclination), 3),
    roll: last.roll,
    leg: true,
    mark: markOf(shots),
    error: shots.map((s) => s.error).find((e) => e) ?? null,
  };
  const steep = Math.abs(reading.inclination) > STEEP_DEG;
  const spread: Spread = {
    distance: tidy(range((r) => r.distance), 4),
    inclination: tidy(range((r) => r.inclination), 3),
    azimuth: steep ? 0 : tidy(Math.max(...shots.map((r) => angleGap(r.azimuth, azimuth))) * 2, 3),
  };
  return { reading, spread };
}

/** The mark of a leg: the last real mark any of its shots carries (the surveyor sets it while the laser is on, on whichever shot). */
function markOf(shots: Reading[]): Mark {
  for (let i = shots.length - 1; i >= 0; i--) {
    const m = shots[i].mark;
    if (m && m !== 'none') return m;
  }
  return 'none';
}

export class LegAssembler {
  private group: Group | null = null;
  private counter = 0;

  /** Takes the next shot, in the order the instrument sent them. Returns what that settled. */
  push(shot: Reading): LegEvent[] {
    const out: LegEvent[] = [];
    const g = this.group;
    if (g && isClose(g.shots[g.shots.length - 1], shot)) {
      g.shots.push(shot);
      if (g.leg) out.push(this.legEvent(g, 'leg-update'));
      else if (g.shots.length >= SHOTS_PER_LEG || shot.leg) {
        g.leg = true;
        out.push(this.legEvent(g, 'leg'));
      }
      return out;
    }
    out.push(...this.settle());
    const fresh: Group = { id: ++this.counter, shots: [shot], leg: false };
    this.group = fresh;
    // A shot the device itself flags as a leg is one, even alone: the device knows what it counted.
    if (shot.leg) {
      fresh.leg = true;
      out.push(this.legEvent(fresh, 'leg'));
    }
    return out;
  }

  /** Nothing more is coming for now (the surveyor paused): what is waiting is splays. */
  flush(): LegEvent[] {
    const out = this.settle();
    this.group = null;
    return out;
  }

  /** Forgets everything (a new connection, another cave). */
  reset() {
    this.group = null;
  }

  /** Shots waiting on a decision (one or two close shots that may yet become a leg). */
  get waiting(): number {
    return this.group && !this.group.leg ? this.group.shots.length : 0;
  }

  private settle(): LegEvent[] {
    const g = this.group;
    this.group = null;
    if (!g || g.leg) return [];
    return g.shots.map((reading) => ({ kind: 'splay' as const, reading }));
  }

  private legEvent(g: Group, kind: 'leg' | 'leg-update'): LegEvent {
    const { reading, spread } = average(g.shots);
    return { kind, group: g.id, reading, shots: g.shots.length, mark: reading.mark ?? 'none', spread, flagged: g.shots.some((s) => s.leg === true) };
  }
}
