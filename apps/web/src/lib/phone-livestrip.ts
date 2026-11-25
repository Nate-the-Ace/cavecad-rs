// The phone Sketch screen's live shot strip, kept pure so it can be checked without a browser (tests/phonelivestrip.check.ts):
// which view the strip shows, the connection dot, the Take shot button and the problem marker.
import type { LinkStatus } from './survey-types.ts';
import { shotEnabled, shotPress, type ShotType } from './phone-survey.ts';

export type StripView = 'connect' | 'held' | 'collapsed' | 'numbers';
export type DotState = 'green' | 'amber' | 'grey';
export type ShotKind = 'single' | 'leg';

export interface StripInput {
  status: LinkStatus;
  /** The user folded the strip away (kept on this device). */
  collapsed: boolean;
  /** Readings waiting for Accept or Discard (Write at once is off). */
  held: number;
  /** An X1: the first press aims the laser, the second shoots. */
  aimsFirst: boolean;
  feature: boolean;
  canTakeShot: boolean;
  /** The session is waiting for a backsight. */
  awaiting: boolean;
  /** There is a To station to shoot a leg to. */
  hasTo: boolean;
  /** Shots of a station-to-station burst still to fire. */
  burst: number;
  /** The laser is on for this kind of shot (the first press was made). */
  aiming: '' | ShotKind;
  message: string;
  warnings: number;
  /** The default shot an X1 takes (Settings > Survey); single when not given. */
  preferred?: ShotType;
  /** Shots are taken at one station only (LRUD mode, a sweep): no leg to shoot. */
  atOnly?: boolean;
}

export interface ShotButton {
  kind: 'shot' | 'feature';
  /** For an X1: which press sequence the button runs. */
  press: ShotKind | null;
  enabled: boolean;
  armed: boolean;
}

export interface StripModel {
  view: StripView;
  dot: DotState;
  /** The dot is a button (to the Connection settings) while the instrument is not connected. */
  dotOpensSettings: boolean;
  /** The view's button when the link is not up: Connect, or Stop while it is trying. */
  connectLabel: 'Connect' | 'Stop' | '';
  shot: ShotButton | null;
  /** Something is wrong or worth a caution: a small marker, no words (they live in the Survey tab). */
  problem: 'problem' | 'caution' | '';
}

export function dotState(status: LinkStatus): DotState {
  return status === 'connected' ? 'green' : status === 'connecting' || status === 'lost' ? 'amber' : 'grey';
}

/** An X1 takes a single shot, or the leg's shot once a backsight is awaited (a single is refused then). */
export const pressKind = (awaiting: boolean): ShotKind => (awaiting ? 'leg' : 'single');

export function stripModel(i: StripInput): StripModel {
  const up = i.status === 'connected';
  const view: StripView = i.held > 0 ? 'held' : !up ? (i.collapsed ? 'collapsed' : 'connect') : i.collapsed ? 'collapsed' : 'numbers';
  let shot: ShotButton | null = null;
  if (up && view !== 'held') {
    if (i.feature) shot = { kind: 'feature', press: null, enabled: i.canTakeShot, armed: false };
    else if (i.aimsFirst) {
      const press = shotPress({ preferred: i.preferred ?? 'single', awaiting: i.awaiting, atOnly: !!i.atOnly, hasTo: i.hasTo });
      const enabled = shotEnabled(press, i);
      shot = { kind: 'shot', press, enabled, armed: i.aiming === press };
    } else shot = { kind: 'shot', press: null, enabled: i.canTakeShot, armed: !!i.aiming };
  }
  return {
    view,
    dot: dotState(i.status),
    dotOpensSettings: !up,
    connectLabel: view === 'connect' ? (i.status === 'connecting' || i.status === 'lost' ? 'Stop' : 'Connect') : '',
    shot,
    problem: i.message.trim() ? 'problem' : i.warnings > 0 ? 'caution' : '',
  };
}

/** A length as its number and its unit, so the unit can be set small ("12.4 m"). */
export function splitUnit(text: string): { value: string; unit: string } {
  const at = text.lastIndexOf(' ');
  return at < 0 ? { value: text, unit: '' } : { value: text.slice(0, at), unit: text.slice(at + 1) };
}
