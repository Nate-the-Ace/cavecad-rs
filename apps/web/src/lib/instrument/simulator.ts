// A pretend instrument, so the whole in-cave workflow can be tried and tested with no hardware.
// It speaks the same wire format as a real one (frames in, acks out, repeats when an ack is lost),
// so everything downstream runs unchanged. The survey it walks is made from a seed, never from the
// clock or from Math.random: the same seed is the same cave.

import { encodeLeg, type Leg } from './caveble.ts';
import { LegLink } from './link.ts';
import type { Instrument, LinkEvents } from './types.ts';

/** What a surveyor does at one point of a traverse. */
export interface Step {
  kind: 'leg' | 'back' | 'splay';
  distance: number;
  azimuth: number;
  inclination: number;
}

/** A small, repeatable random stream (mulberry32). */
export function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const mod360 = (a: number) => ((a % 360) + 360) % 360;
const round = (v: number, d: number) => Math.round(v * 10 ** d) / 10 ** d;

/**
 * The shots of a wandering passage, in the order a surveyor takes them: a foresight per leg, then (when
 * `backsights`) the backsight, then `splays` readings round the new station (left, right, up, down).
 * Readings are rounded the way an instrument shows them, and a backsight reads the opposite way with a
 * little noise, so a closing check on it has something to do.
 */
export function traverse(opts: { seed: number; legs: number; backsights?: boolean; splays?: number }): Step[] {
  const next = rng(opts.seed);
  const out: Step[] = [];
  let heading = next() * 360;
  for (let i = 0; i < opts.legs; i++) {
    heading = mod360(heading + (next() - 0.5) * 70);
    const distance = round(2 + next() * 7, 2);
    const inclination = round((next() - 0.5) * 24, 1);
    out.push({ kind: 'leg', distance, azimuth: round(heading, 1), inclination });
    if (opts.backsights) {
      out.push({
        kind: 'back',
        distance: round(distance + (next() - 0.5) * 0.04, 2),
        azimuth: round(mod360(heading + 180 + (next() - 0.5) * 1.6), 1),
        inclination: round(-inclination + (next() - 0.5) * 1.2, 1),
      });
    }
    const splayAims: [number, number][] = [[-90, 0], [90, 0], [0, 90], [0, -90]];
    for (let s = 0; s < (opts.splays ?? 0); s++) {
      const [turn, up] = splayAims[s % 4];
      out.push({ kind: 'splay', distance: round(0.4 + next() * 2.2, 2), azimuth: round(mod360(heading + turn), 1), inclination: up });
    }
  }
  return out;
}

export interface SimulatorOptions {
  /** The shots it will give, one per `takeShot`. Default: a short traverse with backsights. */
  steps?: Step[];
  /** When true each frame is sent twice before the ack "arrives", as an instrument does when an ack is lost. */
  repeatFrames?: boolean;
}

export class SimulatedInstrument implements Instrument {
  readonly name = 'Simulated instrument';
  readonly canTakeShot = true;
  readonly groupsLegs = false;
  /** Every command byte the surveyor sent, in order (the acks). */
  readonly received: number[] = [];
  private readonly link: LegLink;
  private readonly steps: Step[];
  private index = 0;
  private seq: 0 | 1 = 0;
  private timer: ReturnType<typeof setInterval> | null = null;
  private connected = false;

  private readonly events: LinkEvents;
  private readonly opts: SimulatorOptions;

  constructor(events: LinkEvents, opts: SimulatorOptions = {}) {
    this.events = events;
    this.opts = opts;
    this.steps = opts.steps ?? traverse({ seed: 7, legs: 12, backsights: true });
    this.link = new LegLink(events, (b) => this.received.push(b), 'sim');
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

  /** The step the next shot will give, or `null` when the script has run out. */
  get upcoming(): Step | null {
    return this.steps[this.index] ?? null;
  }

  takeShot() {
    const step = this.steps[this.index];
    if (!this.connected || !step) return;
    this.index += 1;
    const leg: Leg = { seq: this.seq, azimuth: step.azimuth, inclination: step.inclination, roll: 0, distance: step.distance };
    this.seq = this.seq === 0 ? 1 : 0;
    const frame = encodeLeg(leg);
    this.link.onFrame(frame, 'sim');
    if (this.opts.repeatFrames) this.link.onFrame(frame, 'sim');
  }

  /** Fires a shot every `ms` until the script runs out (the demo mode of the Survey panel). */
  auto(ms: number) {
    this.stop();
    this.timer = setInterval(() => {
      if (!this.upcoming) return this.stop();
      this.takeShot();
    }, ms);
  }

  stop() {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }
}
