// A pretend instrument, for any supported device. It makes the shots of a survey (three close shots per leg, one
// per splay, a backsight leg marked backsight), writes each one in the device's OWN wire format (the same encoders
// the tests use) and hands it to the device's real link, so framing, acknowledgement, repeat handling and decoding
// run exactly as for a real unit. Each device has quirks that can be switched on to see how the logic copes before
// there is hardware to find out: a lost acknowledgement, a firmware fault, a missing indication, a calibration shot
// in the middle of a survey. What each quirk is meant to show is in `DEVICES` below and in docs/INSTRUMENTS.md.

import { encodeLeg } from './caveble.ts';
import { encodePacket, type X1Mark } from './cavway.ts';
import { encodeShot } from './bric.ts';
import { encodeDistoXPacket } from './sap5.ts';
import { BricLink, LegLink, Sap5Link, X1Link } from './link.ts';
import { rng, traverse, type Step, type Walls } from './simulator.ts';
import type { Instrument, LinkEvents } from './types.ts';

export type DeviceKind = 'x1' | 'sap6' | 'sap5' | 'bric';

/** A way a device misbehaves, and what to watch for. */
export interface Quirk {
  id: string;
  label: string;
  /** What it should look like when the logic copes. */
  expect: string;
}

export interface Device {
  kind: DeviceKind;
  label: string;
  quirks: Quirk[];
}

const none: Quirk = { id: 'none', label: 'No quirks', expect: 'A clean survey.' };

/** The devices that can be simulated, and the quirks of each. */
export const DEVICES: Device[] = [
  {
    kind: 'x1',
    label: 'Cavway X1',
    quirks: [
      none,
      { id: 'flag-all', label: 'Leg flag on every shot', expect: 'The same survey: a leg is reported at its first shot and refined by the next two.' },
      { id: 'flag-none', label: 'No leg flag', expect: 'The same survey: three close shots still make a leg, by closeness alone.' },
      { id: 'short', label: '33-byte packets', expect: 'The same survey: the short packet form decodes.' },
      { id: 'lost-acks', label: 'Lost acks (packets repeat)', expect: 'The same survey: every repeat is acknowledged but written once.' },
      { id: 'calibration', label: 'Calibration shots mixed in', expect: 'The same survey: calibration packets are acknowledged and left out, and the log says so.' },
      { id: 'accuracy', label: 'Accuracy errors', expect: 'The same survey, with some shots carrying an "accuracy error" warning.' },
    ],
  },
  {
    kind: 'sap6',
    label: 'SAP6',
    quirks: [none, { id: 'lost-acks', label: 'Lost acks (frames repeat)', expect: 'The same survey: each repeat is acknowledged again and written once.' }],
  },
  {
    kind: 'sap5',
    label: 'SAP5',
    quirks: [
      none,
      { id: 'fault32', label: 'Long shots (firmware bit-16 fault)', expect: 'Shots past 32.768 m carry the SAP5 firmware fault; they must still read their true length, not 65 m too long.' },
      { id: 'repeats', label: 'Repeated packets', expect: 'The same survey: a repeated packet is written once.' },
    ],
  },
  {
    kind: 'bric',
    label: 'BRIC5',
    quirks: [
      none,
      { id: 'no-errors', label: 'No error indication', expect: 'The same survey: each shot is written when the next begins, or after 400 ms.' },
      { id: 'missed', label: 'Missed shots (index jumps)', expect: 'The same survey, and the log says shots were missed.' },
      { id: 'scan', label: 'Scan shots (splays)', expect: 'The same survey: scan shots (type 1) are ordinary readings.' },
      { id: 'zero', label: 'Zero-length shots', expect: 'The same survey: shots with no length are ignored, and the log says so.' },
      { id: 'errors', label: 'Instrument errors', expect: 'The same survey, with some shots carrying "the target moved".' },
      { id: 'repeats', label: 'Repeated indications', expect: 'The same survey: a repeated primary is written once.' },
    ],
  },
];

export const deviceOf = (kind: DeviceKind): Device => DEVICES.find((d) => d.kind === kind)!;

/** `all`: every shot of a leg is flagged; `third`: only the third is; `none`: the flag is never set (an X1 only). */
export type FlagPolicy = 'all' | 'third' | 'none';

export interface ShotSpec {
  distance: number;
  azimuth: number;
  inclination: number;
  /** Flagged as part of a leg by the device (an X1 only; depends on the flag policy). */
  leg: boolean;
  /** Part of a leg whatever the flag says: for the quirks that need to know. */
  isLeg: boolean;
  mark: X1Mark;
  /** A shot that measures this wall of the station (the surveyor has the LRUD mode on): never a splay. */
  lrud?: 'left' | 'right' | 'up' | 'down';
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
    if (s.kind === 'splay' || s.kind === 'lrud') {
      out.push({ distance: s.distance, azimuth: s.azimuth, inclination: s.inclination, leg: false, isLeg: false, mark: 'none', ...(s.kind === 'lrud' ? { lrud: s.side } : {}) });
      continue;
    }
    for (let i = 0; i < 3; i++) {
      out.push({
        // Close but not identical: a few millimetres and a fraction of a degree, as three real shots are.
        distance: round(s.distance + (next() - 0.5) * 0.006, 3),
        azimuth: round(((s.azimuth + (next() - 0.5) * 0.4) % 360 + 360) % 360, 2),
        inclination: round(s.inclination + (next() - 0.5) * 0.4, 2),
        leg: policy === 'all' || (policy === 'third' && i === 2),
        isLeg: true,
        mark: s.kind === 'back' ? 'backsight' : 'none',
        walls: i === 0 && s.kind === 'leg' ? s.walls : undefined,
        behind: i === 0 && s.kind === 'leg' ? s.behind : undefined,
      });
    }
  }
  return out;
}

export interface SimOptions {
  kind: DeviceKind;
  steps?: Step[];
  /** X1 only; a quirk of `flag-all` or `flag-none` overrides it. */
  policy?: FlagPolicy;
  /** The quirks switched on (any of the device's `quirks` ids). */
  quirks?: string[];
}

const ACCURACY_ERRORS = [0x6f, 0x04, 0x29, 0x1c, 0x25, 0x1c, 0x07, 0x00, 0x00]; // an absG error and an angle error

export class SimulatedInstrument implements Instrument {
  readonly name: string;
  readonly kind: DeviceKind;
  /** The simulator always has a button to press, whatever the real device can do. */
  readonly canTakeShot = true;
  readonly groupsLegs = true;
  /** Every message the surveyor wrote to the device (the acknowledgements), in order. */
  readonly received: Uint8Array[] = [];
  /** Called when the script has run out while firing on its own. */
  onFinished: (() => void) | null = null;
  /** Called after a whole station has been shot: the surveyor stops, so what is waiting is settled. */
  onBurst: (() => void) | null = null;
  /** Whether the surveyor is taking backsights right now (asked at each press, so the switch can change mid-survey). */
  wantBacksights: () => boolean = () => true;
  /** Whether the wall shots (LRUD) are being taken right now. */
  wantLrud: () => boolean = () => true;
  private readonly events: LinkEvents;
  private readonly quirks: Set<string>;
  private readonly shots: ShotSpec[];
  private readonly x1: X1Link;
  private readonly sap6: LegLink;
  private readonly sap5: Sap5Link;
  private readonly bric: BricLink;
  private index = 0;
  /** Which wall each shot fired measures, in the order fired: put on its reading when it comes out of the decoder (a BRIC's comes late). */
  private readonly walls: (ShotSpec['lrud'])[] = [];
  private fired = 0;
  private seq: 0 | 1 = 0;
  private bricIndex = 0;
  private hint: { at: Walls; behind: Walls } | null = null;
  private behind: Walls | null = null;
  private clock = 1_700_000_000;
  private timer: ReturnType<typeof setInterval> | null = null;
  private connected = false;

  constructor(events: LinkEvents, opts: SimOptions) {
    this.events = events;
    this.kind = opts.kind;
    this.name = `Simulated ${deviceOf(opts.kind).label}`;
    this.quirks = new Set(opts.quirks ?? []);
    const policy: FlagPolicy = this.quirks.has('flag-all') ? 'all' : this.quirks.has('flag-none') ? 'none' : (opts.policy ?? 'third');
    this.shots = shotsFor(opts.steps ?? traverse({ seed: 7, legs: 20, backsights: true, splays: 2 }), policy);
    // The SAP5 fault shows on long shots: a survey of legs several times the length shows it.
    if (this.quirks.has('fault32')) for (const s of this.shots) if (s.isLeg) s.distance = round(s.distance * 8, 3);
    // An instrument cannot say that a shot is a wall dimension: the surveyor does, so the reading is marked after it is decoded.
    const marked: LinkEvents = { ...events, status: (s, d) => events.status(s, d), frame: (h, f) => events.frame(h, f), reading: (r) => { const lrud = this.walls.shift(); events.reading(lrud ? { ...r, lrud } : r); } };
    this.x1 = new X1Link(marked, (b) => this.received.push(b), 'sim');
    this.sap6 = new LegLink(marked, (b) => this.received.push(Uint8Array.of(b)), 'sim');
    this.sap5 = new Sap5Link(marked, 'sim');
    this.bric = new BricLink(marked, 'sim');
  }

  async connect() {
    this.events.status('connecting');
    this.connected = true;
    this.events.status('connected', this.name);
  }

  disconnect() {
    this.stop();
    this.walls.length = 0;
    this.bric.flush();
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

  /** Fires one shot. */
  takeShot() {
    const s = this.shots[this.index];
    if (!this.connected || !s) return;
    this.index += 1;
    this.fired += 1;
    this.clock += 1;
    this.walls.push(s.lrud);
    if (s.walls) {
      // A new leg: the passage at the station it reaches, and (for the first leg) at the one it leaves.
      this.hint = { at: s.walls, behind: s.behind ?? this.behind ?? s.walls };
      this.behind = s.walls;
    }
    if (this.kind === 'x1') this.sendX1(s);
    else if (this.kind === 'sap6') this.sendSap6(s);
    else if (this.kind === 'sap5') this.sendSap5(s);
    else this.sendBric(s);
  }

  private sendX1(s: ShotSpec) {
    if (this.quirks.has('calibration') && this.fired % 6 === 0) {
      this.x1.onFrame(encodePacket({ distance: 0, azimuth: 295, inclination: 8, type: 2, time: this.clock }), 'sim');
    }
    const errors = this.quirks.has('accuracy') && this.fired % 5 === 0 ? ACCURACY_ERRORS : undefined;
    const packet = encodePacket({ ...s, time: this.clock, short: this.quirks.has('short'), errors });
    this.x1.onFrame(packet, 'sim');
    if (this.quirks.has('lost-acks')) this.x1.onFrame(packet, 'sim'); // no ack heard: the X1 sends it again
  }

  private sendSap6(s: ShotSpec) {
    const frame = encodeLeg({ seq: this.seq, azimuth: s.azimuth, inclination: s.inclination, roll: 0, distance: s.distance });
    this.seq = this.seq === 0 ? 1 : 0;
    this.sap6.onFrame(frame, 'sim');
    if (this.quirks.has('lost-acks')) this.sap6.onFrame(frame, 'sim');
  }

  private sendSap5(s: ShotSpec) {
    const packet = encodeDistoXPacket({ distance: s.distance, azimuth: s.azimuth, inclination: s.inclination, roll: 0, seq: this.seq });
    this.seq = this.seq === 0 ? 1 : 0;
    // The firmware fault: bit 16 of the distance is set once a distance passes 32.768 m (not 65.536 m).
    if (this.quirks.has('fault32') && s.distance >= 32.768 && s.distance < 65.536) packet[0] |= 0x40;
    this.sap5.onFrame(packet, 'sim');
    if (this.quirks.has('repeats')) this.sap5.onFrame(packet, 'sim');
  }

  private sendBric(s: ShotSpec) {
    const bric = (distance: number, type: number, errors?: [number, number, number][]) => {
      this.bricIndex += this.quirks.has('missed') && this.fired % 7 === 0 ? 2 : 1;
      const sec = this.clock % 60;
      const min = Math.floor(this.clock / 60) % 60;
      const shot = encodeShot({ distance, azimuth: s.azimuth, inclination: s.inclination, index: this.bricIndex, type, errors, time: { minute: min, second: sec } });
      this.bric.onFrame('primary', shot.primary);
      if (this.quirks.has('repeats')) this.bric.onFrame('primary', shot.primary); // the same indication again
      this.bric.onFrame('meta', shot.meta);
      if (!this.quirks.has('no-errors')) this.bric.onFrame('errors', shot.errors);
    };
    const errors: [number, number, number][] | undefined = this.quirks.has('errors') && this.fired % 5 === 0 ? [[7, 0, 0]] : undefined;
    bric(s.distance, this.quirks.has('scan') && !s.isLeg ? 1 : 0, errors);
    if (this.quirks.has('zero') && this.fired % 9 === 0) {
      this.clock += 1;
      bric(0, 0); // a mis-fire: a shot with no length
    }
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
      if (next?.lrud && !this.wantLrud()) {
        this.index += 1; // no LRUD today: that wall is not shot
        continue;
      }
      if (next?.walls) sawLeg = true;
      this.takeShot();
      fired += 1;
      // A foresight leg's first shot carries the passage at its far end: the one after the leg this call shot starts the next station.
    } while (this.upcoming && !(sawLeg && this.upcoming.walls));
    this.bric.flush(); // the surveyor stops: a BRIC shot still waiting for its errors is written
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
