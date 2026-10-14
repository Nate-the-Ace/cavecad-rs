// A pretend Cavway X1. One press of "take shot" gives ONE shot, as on the real unit: a leg is three close
// shots, a splay one, a backsight leg three shots marked backsight. The packets are built by `encodePacket`
// (the same layout the decoder reads) and go through `X1Link`, so framing, acknowledgement, repeat handling
// and decoding are exercised exactly as for a real instrument; the grouping into legs happens downstream.
//
// What the unit does with the leg flag is not known yet (docs/CAVWAY-X1.md), so the simulator can behave
// three ways and the tests run all of them.

import { encodePacket, type X1Mark } from './cavway.ts';
import { X1Link } from './link.ts';
import { rng, traverse, type Step, type Walls } from './simulator.ts';
import type { Instrument, LinkEvents } from './types.ts';

/** `all`: every shot of a leg is flagged; `third`: only the third is; `none`: the flag is never set. */
export type FlagPolicy = 'all' | 'third' | 'none';

export interface ShotSpec {
  distance: number;
  azimuth: number;
  inclination: number;
  leg: boolean;
  mark: X1Mark;
  /** The passage at the station a leg arrives at (carried on the first shot of a foresight leg). */
  walls?: Walls;
  /** On the first leg, when the survey starts with splays: the passage at the station it leaves. */
  behind?: Walls;
}

const round = (v: number, d: number) => Math.round(v * 10 ** d) / 10 ** d;

/** The single shots a surveyor produces for these steps: three close shots per leg, one per splay. */
export function shotsFor(steps: Step[], policy: FlagPolicy, seed = 1): ShotSpec[] {
  const next = rng(seed);
  const out: ShotSpec[] = [];
  for (const s of steps) {
    if (s.kind === 'splay') {
      out.push({ distance: s.distance, azimuth: s.azimuth, inclination: s.inclination, leg: false, mark: 'none' });
      continue;
    }
    for (let i = 0; i < 3; i++) {
      out.push({
        // Close but not identical: a few millimetres and a fraction of a degree, as three real shots are.
        distance: round(s.distance + (next() - 0.5) * 0.006, 3),
        azimuth: round(((s.azimuth + (next() - 0.5) * 0.4) % 360 + 360) % 360, 2),
        inclination: round(s.inclination + (next() - 0.5) * 0.4, 2),
        leg: policy === 'all' || (policy === 'third' && i === 2),
        mark: s.kind === 'back' ? 'backsight' : 'none',
        walls: i === 0 && s.kind === 'leg' ? s.walls : undefined,
        behind: i === 0 && s.kind === 'leg' ? s.behind : undefined,
      });
    }
  }
  return out;
}

export interface X1SimOptions {
  steps?: Step[];
  policy?: FlagPolicy;
  /** Each packet is delivered twice, as the X1 does when it has not heard the ack. */
  repeatFrames?: boolean;
  /** Send the 33-byte short packet instead of 64. */
  short?: boolean;
}

export class X1SimulatedInstrument implements Instrument {
  readonly name = 'Simulated Cavway X1';
  readonly canTakeShot = true;
  readonly groupsLegs = true;
  /** Every framed message the surveyor wrote (the acks), in order. */
  readonly received: Uint8Array[] = [];
  private readonly link: X1Link;
  private readonly shots: ShotSpec[];
  private readonly opts: X1SimOptions;
  private readonly events: LinkEvents;
  /** Called when the script has run out while firing on its own. */
  onFinished: (() => void) | null = null;
  /** Called after a whole station has been shot: the surveyor stops, so what is waiting is settled. */
  onBurst: (() => void) | null = null;
  /** Whether the surveyor is taking backsights right now (asked at each press, so the switch can change mid-survey). */
  wantBacksights: () => boolean = () => true;
  private index = 0;
  private hint: { at: Walls; behind: Walls } | null = null;
  private behind: Walls | null = null;
  private clock = 1_700_000_000;
  private timer: ReturnType<typeof setInterval> | null = null;
  private connected = false;

  constructor(events: LinkEvents, opts: X1SimOptions = {}) {
    this.events = events;
    this.opts = opts;
    this.shots = shotsFor(opts.steps ?? traverse({ seed: 7, legs: 20, backsights: true, splays: 2 }), opts.policy ?? 'third');
    this.link = new X1Link(events, (b) => this.received.push(b), 'sim');
  }

  async connect() {
    this.events.status('connecting');
    this.connected = true;
    this.events.status('connected', this.name);
  }

  disconnect() {
    this.stop();
    this.connected = false;
    this.events.status('idle');
  }

  /** The passage at the station of the leg being shot, and at the one behind it: what a surveyor would measure there. */
  passage(): { at: Walls; behind: Walls } | null {
    return this.hint;
  }

  /** The shot the next press will give, or `null` when the script has run out. */
  get upcoming(): ShotSpec | null {
    return this.shots[this.index] ?? null;
  }

  takeShot() {
    const s = this.shots[this.index];
    if (!this.connected || !s) return;
    this.index += 1;
    this.clock += 1;
    if (s.walls) {
      // A new leg: the passage at the station it reaches, and (for the first leg) at the one it leaves.
      this.hint = { at: s.walls, behind: s.behind ?? this.behind ?? s.walls };
      this.behind = s.walls;
    }
    const packet = encodePacket({ ...s, time: this.clock, short: this.opts.short });
    this.link.onFrame(packet, 'sim');
    if (this.opts.repeatFrames) this.link.onFrame(packet, 'sim');
  }

  /**
   * Shoots the next whole station in one go, as a surveyor would: any splays still to be shot where they stand (the
   * first station's), the leg (three close shots), its backsight when the script has one, and the splays that measure
   * the passage at the new station. Stops before the next leg. Returns how many shots it fired.
   */
  takeStation(): number {
    if (!this.connected || !this.upcoming) return 0;
    let fired = 0;
    let sawLeg = false;
    do {
      const next = this.upcoming;
      if (next?.mark === 'backsight' && !this.wantBacksights()) {
        this.index += 1; // no backsights today: that leg's backsight is not shot
        continue;
      }
      if (next?.walls) sawLeg = true;
      this.takeShot();
      fired += 1;
      // A foresight leg's first shot carries the passage at its far end: the one after the leg this call shot starts the next station.
    } while (this.upcoming && !(sawLeg && this.upcoming.walls));
    this.onBurst?.();
    return fired;
  }

  /** Shoots a whole station every `ms` until the script runs out (the demo mode of the Survey panel). */
  auto(ms: number) {
    this.stop();
    this.timer = setInterval(() => {
      if (this.upcoming) this.takeStation();
      else {
        this.stop();
        this.onFinished?.();
      }
    }, ms);
  }

  stop() {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }
}
