// The Survey session's rules, with no screen and no engine in them: given what the surveyor has set
// (leg or splay, backsights or not, where they stand) and a reading from the instrument, what should be
// written to the notebook, and where do they stand afterwards. Pure, so every rule is tested
// (tests/survey.check.ts). The panel and `survey.svelte.ts` only carry these decisions out.
//
// What the instrument sends is never changed on the way in: the compass reading stays what the compass
// said (the notebook turns it into a true bearing with the trip's declination), a backsight keeps its
// own sign and direction, and a steep shot keeps whatever bearing the instrument gave (the engine
// already ignores the compass on a pitch, so nothing here invents or drops one).

import { suggestNext } from './notebook.ts';
import type { LegEvent } from './instrument/legs.ts';
import type { Reading } from './instrument/types.ts';

/** `auto`: the instrument says what each shot is (an X1: three close shots are a leg). `leg` and `splay`: the surveyor says. */
export type Mode = 'auto' | 'leg' | 'splay';

/** A foresight has been written and its backsight is the next reading. */
export interface Awaiting {
  shot: string;
  /** The foresight, to compare the backsight against. */
  reading: Reading;
}

export interface SessionState {
  mode: Mode;
  /** Take a backsight after each foresight. */
  backsights: boolean;
  /** The station the surveyor stands at (a leg starts here, a splay is taken from here). */
  from: string;
  /** The station the next leg goes to. */
  to: string;
  awaiting: Awaiting | null;
  /** The last leg written: what a backsight marked on the instrument belongs to when none is awaited. */
  last: Awaiting | null;
}

/** What to write to the notebook. */
export type Plan =
  | { kind: 'leg'; from: string; to: string; reading: Reading }
  | { kind: 'splay'; from: string; reading: Reading }
  | { kind: 'back'; shot: string; reading: Reading };

export interface Decision {
  plan: Plan | null;
  /** Why the reading cannot be used at all (nothing is written). */
  reject: string | null;
  /** Worth a look, but written anyway. */
  warnings: string[];
}

/** Longest shot taken as believable, metres. A laser reading past this is a glitch. */
export const MAX_DISTANCE_M = 600;
/** Shots shorter than this are probably a mis-fire, metres. */
export const MIN_DISTANCE_M = 0.05;
/** The engine reads a shot steeper than this as a plumbed pitch and ignores its compass. */
export const PLUMB_DEG = 85;
/** Degrees by which foresight and backsight may disagree (the same figure the notebook flags at). */
export const BACKSIGHT_TOLERANCE_DEG = 3;
/** Tape difference between foresight and backsight that is worth saying: larger of 0.1 m and 2 %. */
export const TAPE_TOLERANCE_M = 0.1;
export const TAPE_TOLERANCE_FRACTION = 0.02;

const mod360 = (a: number) => ((a % 360) + 360) % 360;
/** Smallest angle between two bearings, degrees (0 to 180). */
const gap = (a: number, b: number) => Math.abs(((((a - b) % 360) + 540) % 360) - 180);

/**
 * A reading that can be written as it is, or the reason it cannot. A shot steeper than the plumb limit is flagged for a
 * leg (the engine reads it as a pitch and ignores the compass), but not for a splay: up and down splays are how a
 * passage is measured, and say nothing the surveyor needs told.
 */
export function checkReading(r: Reading, splay = false): { ok: true; warnings: string[] } | { ok: false; why: string } {
  if (![r.distance, r.azimuth, r.inclination].every(Number.isFinite)) return { ok: false, why: 'The instrument sent something that is not a number.' };
  if (r.distance <= 0) return { ok: false, why: 'The tape reading is zero or negative.' };
  if (r.distance > MAX_DISTANCE_M) return { ok: false, why: 'The tape reading is far too long to be right.' };
  if (r.inclination < -90 || r.inclination > 90) return { ok: false, why: 'The clino reading is past vertical.' };
  const warnings: string[] = [];
  if (r.distance < MIN_DISTANCE_M) warnings.push('Very short shot: check it was not a mis-fire.');
  if (!splay && Math.abs(r.inclination) > PLUMB_DEG) warnings.push(`Steeper than ${PLUMB_DEG}°: read as a pitch, so the compass is ignored.`);
  return { ok: true, warnings };
}

/** What a backsight says about its foresight: the same checks the notebook flags a page with. */
export function backsightWarnings(fore: Reading, back: Reading): string[] {
  const out: string[] = [];
  // Steep shots have no meaningful compass, so only the clino is compared.
  if (Math.abs(fore.inclination) <= PLUMB_DEG && Math.abs(back.inclination) <= PLUMB_DEG) {
    const g = gap(fore.azimuth, mod360(back.azimuth + 180));
    if (g > BACKSIGHT_TOLERANCE_DEG) out.push(`Compass foresight and backsight differ by ${g.toFixed(1)}°.`);
  }
  const sum = Math.abs(fore.inclination + back.inclination);
  if (sum > BACKSIGHT_TOLERANCE_DEG) out.push(`Clino foresight and backsight differ by ${sum.toFixed(1)}°.`);
  const d = Math.abs(fore.distance - back.distance);
  if (d > Math.max(TAPE_TOLERANCE_M, TAPE_TOLERANCE_FRACTION * fore.distance)) out.push(`Tape differs by ${d.toFixed(2)} m between foresight and backsight.`);
  return out;
}

/** How far a leg may be from exactly reversed to be taken as the backsight of the leg before it, degrees (compass and clino each). */
export const BACK_FIND_DEG = 25;

/**
 * True when `r` looks like the backsight of `fore`: it points back along it (the compass about 180 degrees round, the
 * clino about opposite) and is about as long. Only the X1 can mark a leg as a backsight on the instrument; the others
 * (SAP5, SAP6, BRIC) cannot, so after a leg with backsights switched on the surveyor's next leg is taken as its
 * backsight if it comes back, and as the next leg if it does not (the backsight was skipped).
 */
export function looksLikeBack(fore: Reading, r: Reading): boolean {
  const steep = Math.abs(fore.inclination) > PLUMB_DEG && Math.abs(r.inclination) > PLUMB_DEG;
  if (!steep && gap(fore.azimuth, mod360(r.azimuth + 180)) > BACK_FIND_DEG) return false;
  if (Math.abs(fore.inclination + r.inclination) > BACK_FIND_DEG) return false;
  return Math.abs(fore.distance - r.distance) <= Math.max(1, 0.25 * fore.distance);
}

/** Decides what a reading is: the next leg, a splay, or the backsight being waited for. */
export function decide(state: SessionState, reading: Reading): Decision {
  const checked = checkReading(reading, state.mode === 'splay' && !state.awaiting);
  if (!checked.ok) return { plan: null, reject: checked.why, warnings: [] };
  const warnings = [...checked.warnings];
  if (state.awaiting) {
    warnings.push(...backsightWarnings(state.awaiting.reading, reading));
    return { plan: { kind: 'back', shot: state.awaiting.shot, reading }, reject: null, warnings };
  }
  const from = state.from.trim();
  const to = state.to.trim();
  if (!from) return { plan: null, reject: 'Say which station you are standing at.', warnings };
  if (state.mode === 'splay') return { plan: { kind: 'splay', from, reading }, reject: null, warnings };
  if (!to) return { plan: null, reject: 'Name the station this shot goes to.', warnings };
  if (from === to) return { plan: null, reject: 'A shot joins two different stations.', warnings };
  return { plan: { kind: 'leg', from, to, reading }, reject: null, warnings };
}

/** The surveyor moves on: they stand at `to`, and the next leg goes to the next name in the sequence. */
export function advance(state: SessionState, taken: ReadonlySet<string>): SessionState {
  const stand = state.to.trim();
  const names = new Set(taken);
  names.add(stand);
  return { ...state, from: stand, to: suggestNext(stand, names), awaiting: null };
}

/** The state once `plan` has been written to the notebook (`shot` is the id of the shot it created or changed). */
export function applied(state: SessionState, plan: Plan, shot: string, taken: ReadonlySet<string>): SessionState {
  switch (plan.kind) {
    case 'leg': {
      const made = { shot, reading: plan.reading };
      // With backsights the surveyor has not moved yet: the backsight is taken from the new station
      // looking back. They move on once it is in (or skipped).
      return state.backsights ? { ...state, awaiting: made, last: made } : { ...advance(state, taken), last: made };
    }
    case 'back':
      // A backsight that was awaited moves them on; one marked on the instrument for a leg already left behind does not.
      return state.awaiting ? advance(state, taken) : state;
    case 'splay':
      return state;
  }
}

/** Gives up on the backsight being waited for and moves on. */
export function skipBack(state: SessionState, taken: ReadonlySet<string>): SessionState {
  return state.awaiting ? advance(state, taken) : state;
}

/**
 * Switches between leg and splay. A surveyor who changes mode while a backsight is awaited has given
 * it up and is already at the new station, so they move on (a splay taken now belongs to that station).
 */
export function withMode(state: SessionState, mode: Mode, taken: ReadonlySet<string>): SessionState {
  if (state.mode === mode) return state;
  return { ...(state.awaiting ? advance(state, taken) : state), mode };
}

/** The station the surveyor is physically at: the new one while its backsight is awaited. */
export function standing(state: SessionState): string {
  return (state.awaiting ? state.to : state.from).trim();
}

/** What to tell the surveyor to do next, in plain words. */
export function nextStep(state: SessionState): string {
  if (state.mode === 'auto') {
    if (state.awaiting) return `Backsight: shoot back from ${state.to.trim()} to ${state.from.trim()} (three shots; an X1 can mark them Backsight with M). Or carry on without.`;
    const from = state.from.trim();
    const to = state.to.trim();
    if (!from || !to) return 'Name the two stations, then shoot the leg: three close shots.';
    return `Shoot ${from} to ${to}: three close shots make a leg. A single shot is a splay at ${from}.`;
  }
  if (state.awaiting) return `Take the backsight from ${state.to.trim()} back to ${state.from.trim()}.`;
  if (state.mode === 'splay') return `Take a splay from ${state.from.trim() || '(name the station)'}.`;
  if (!state.from.trim() || !state.to.trim()) return 'Name the two stations, then take the shot.';
  return `Take the shot ${state.from.trim()} to ${state.to.trim()}.`;
}

/** A fresh state standing at `from` (the last station of the page, or the first of a new one). */
export function start(from: string, taken: ReadonlySet<string>, keep?: Pick<SessionState, 'mode' | 'backsights'>): SessionState {
  const names = new Set(taken);
  names.add(from);
  return { mode: keep?.mode ?? 'leg', backsights: keep?.backsights ?? false, from, to: suggestNext(from, names), awaiting: null, last: null };
}

/** Spread among the shots of a leg beyond which it is worth a word, metres and degrees. */
const SPREAD_DISTANCE_M = 0.03;
const SPREAD_ANGLE_DEG = 1.5;

/** The instrument's own events (what the X1 says is a leg or a splay), made into a plan. `base` is where the surveyor stands when it is written (they have moved on if a backsight was awaited and something else came). */
export function decideEvent(state: SessionState, ev: Extract<LegEvent, { kind: 'leg' | 'splay' }>, taken: ReadonlySet<string>): { base: SessionState; decision: Decision } {
  const reading = ev.reading;
  const checked = checkReading(reading, ev.kind === 'splay');
  if (!checked.ok) return { base: state, decision: { plan: null, reject: checked.why, warnings: [] } };
  const warnings = [...checked.warnings];
  if (reading.error) warnings.push(`The instrument reports an ${reading.error}.`);
  const refuse = (base: SessionState, why: string) => ({ base, decision: { plan: null, reject: why, warnings } });
  if (ev.kind === 'splay') {
    const base = state.awaiting ? advance(state, taken) : state;
    const from = base.from.trim();
    if (!from) return refuse(base, 'Say which station you are standing at.');
    return { base, decision: { plan: { kind: 'splay', from, reading }, reject: null, warnings } };
  }
  const { spread } = ev;
  if (spread.distance > SPREAD_DISTANCE_M || spread.azimuth > SPREAD_ANGLE_DEG || spread.inclination > SPREAD_ANGLE_DEG) {
    warnings.push(`The shots of this leg differ by ${spread.distance.toFixed(3)} m, ${spread.azimuth.toFixed(1)}\u00b0 compass, ${spread.inclination.toFixed(1)}\u00b0 clino.`);
  }
  if (ev.mark === 'backsight') {
    const target = state.awaiting ?? state.last;
    if (!target) return refuse(state, 'A backsight needs a leg before it.');
    warnings.push(...backsightWarnings(target.reading, reading));
    return { base: state, decision: { plan: { kind: 'back', shot: target.shot, reading }, reject: null, warnings } };
  }
  // An instrument that cannot mark a backsight: a leg that comes back along the one before is its backsight.
  if (state.awaiting && looksLikeBack(state.awaiting.reading, reading)) {
    warnings.push(...backsightWarnings(state.awaiting.reading, reading));
    return { base: state, decision: { plan: { kind: 'back', shot: state.awaiting.shot, reading }, reject: null, warnings } };
  }
  const base = state.awaiting ? advance(state, taken) : state;
  const from = base.from.trim();
  const to = base.to.trim();
  if (!from) return refuse(base, 'Say which station you are standing at.');
  if (!to) return refuse(base, 'Name the station this leg goes to.');
  if (from === to) return refuse(base, 'A leg joins two different stations.');
  return { base, decision: { plan: { kind: 'leg', from, to, reading }, reject: null, warnings } };
}
