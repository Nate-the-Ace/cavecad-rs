// What the Rust survey crates hand across (JSON): the shapes of `cave-instrument`, `cave-session` and the
// wasm door in `cave-wasm/src/survey.rs`. No logic lives here, only the types.

/** One shot ready for the survey: metres and degrees, as the instrument measured them. */
export interface Reading {
  distance: number;
  /** Degrees, as the instrument reports it (magnetic: the notebook applies the trip's declination). */
  azimuth: number;
  /** Degrees, up positive. */
  inclination: number;
  roll: number | null;
  source: string;
  leg?: boolean;
  mark?: 'none' | 'feature' | 'ridge' | 'backsight' | 'generic';
  error?: string;
  quality?: number;
  lrud?: 'left' | 'right' | 'up' | 'down';
}

export type LinkStatus = 'idle' | 'connecting' | 'connected' | 'lost' | 'unsupported' | 'error';
export type Mode = 'auto' | 'leg' | 'splay' | 'lrud';
export type Side = 'left' | 'right' | 'up' | 'down';

export interface Awaiting {
  shot: string;
  reading: Reading;
}

/** Where the surveyor stands and what they are doing. */
export interface SessionState {
  mode: Mode;
  backsights: boolean;
  from: string;
  to: string;
  awaiting: Awaiting | null;
  last: Awaiting | null;
}

export interface LogEntry {
  n: number;
  kind: 'leg' | 'splay' | 'lrud' | 'back';
  label: string;
  reading: Reading;
  warnings: string[];
}

export interface HeldView {
  text: string;
  warnings: string[];
  reject: string | null;
  reading: Reading | null;
}

export interface StandingWalls {
  station: string;
  left: number | null;
  right: number | null;
  up: number | null;
  down: number | null;
}

/** The session as the screen shows it. */
export interface Snapshot {
  trip: string | null;
  state: SessionState;
  instruction: string;
  /** The station the surveyor is physically at. */
  standing: string;
  auto: boolean;
  groups_legs: boolean;
  held: HeldView[];
  waiting: number;
  last: Reading | null;
  log: LogEntry[];
  message: string;
  warnings: string[];
  standing_walls: StandingWalls | null;
  edits: number;
}

/** What every call into the survey engine answers. */
export interface Step {
  frames: { hex: string; from: string }[];
  /** Bytes to write back to the instrument, in order. */
  sends: number[][];
  snapshot: Snapshot | null;
  error: string | null;
  ok: boolean;
  /** A shot is waiting for the rest of its indications: flush the link after this many ms. */
  settle_ms: number | null;
  sim_left: boolean;
  sim_note: string | null;
}

export interface Quirk {
  id: string;
  label: string;
  expect: string;
}
export interface Device {
  kind: 'x1' | 'sap6' | 'sap5' | 'bric';
  label: string;
  quirks: Quirk[];
}
export type InstrumentKind = 'x1' | 'sap5' | 'sap6' | 'bric' | 'unsupported';
export interface Identified {
  kind: InstrumentKind;
  label: string;
  how: 'name' | 'service';
}

/** The instruments' Bluetooth ids, names and commands, and the simulator's devices: read from Rust, so the browser keeps no copy. */
export interface Catalog {
  devices: Device[];
  names: { prefix: string; kind: InstrumentKind; label: string }[];
  x1: { service: string; write: string; notify: string; takeShot: number[] };
  sap5: { service: string; write: string; notify: string };
  sap6: { service: string; name: string; command: string; leg: string; takeShot: number[] };
  bric: { measureService: string; primary: string; meta: string; errors: string; controlService: string; control: string; takeShot: number[]; batteryService: string; deviceInfoService: string; settleMs: number };
}

/** The part of the survey engine the Bluetooth transport talks to (the real engine, or a stand-in in tests). */
export interface LinkBridge {
  catalog(): Catalog;
  identify(name: string, services: string[]): { identified: Identified | null; nordicUart: boolean };
  connect(kind: string): Step;
  frame(kind: string, bytes: Uint8Array): Step;
  flushLink(): Step;
}

/** What a transport tells the store. */
export interface LinkEvents {
  status(status: LinkStatus, detail?: string): void;
  /** A line for the capture log that did not come from the engine (a capture-mode frame, a note). */
  frame(hex: string, from: string): void;
  /** The engine's answer to a frame (its log lines, its snapshot). The transport has already written its `sends`. */
  step(step: Step): void;
}

/** A way of reaching an instrument. */
export interface Instrument {
  readonly name: string;
  readonly canTakeShot: boolean;
  connect(): Promise<void>;
  disconnect(): void;
  takeShot(): void;
}
