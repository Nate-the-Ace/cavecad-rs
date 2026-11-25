// The phone Survey screen's words, kept pure so they can be checked without a browser (tests/phonesurvey.check.ts).
// The screen has no helper text: a status pill, and at most two short lines when something is wrong.
import type { LinkStatus, SweepKind } from './survey-types.ts';

/** A link that is up or trying to be (Disconnect is the button then, not Connect). */
export const isLinked = (status: LinkStatus): boolean => status === 'connected' || status === 'connecting' || status === 'lost';

/** Whether the link's detail is worth a line: not when connected (it is the name) or idle (a cancel, a disconnect). */
export const showsDetail = (status: LinkStatus): boolean => status !== 'connected' && status !== 'idle';

/** The status pill's text: the instrument's name once connected, otherwise what the link is doing. */
export function linkLabel(status: LinkStatus, device: string): string {
  switch (status) {
    case 'connected':
      return device || 'Connected';
    case 'connecting':
      return 'Connecting…';
    case 'lost':
      return 'Reconnecting…';
    case 'error':
      return 'Could not connect';
    case 'unsupported':
      return 'No Bluetooth here';
    default:
      return 'No instrument';
  }
}

/** What a sweep of this kind is called under each standard. */
export const sweepName = (kind: SweepKind, manual: boolean): string =>
  manual ? (kind === 'lrud' ? 'LRUD' : 'Room outline') : kind === 'lrud' ? 'LRUD ring' : 'Room sweep';

export interface ProblemInput {
  /** The session's problem (a refused reading, a burst that did not come back, ...). */
  message: string;
  /** Why the link is down (shown while it is failing or trying again; not once idle, which is a cancel or a disconnect). */
  detail: string;
  status: LinkStatus;
  /** The instrument's accuracy complaints and the session's cautions, already in words. */
  warnings: string[];
  /** The browser cannot reach Bluetooth at all. */
  noBluetooth: boolean;
  /** The station needs a sweep before moving on (Digital), as its label, or ''. */
  sweepDue: string;
}

export interface ProblemLine {
  text: string;
  kind: 'problem' | 'caution';
}

/** The screen's problem lines, worst first, never more than two (the line shows only when something is wrong). */
export function problemLines(p: ProblemInput): ProblemLine[] {
  const out: ProblemLine[] = [];
  const add = (text: string, kind: ProblemLine['kind']) => {
    const t = text.trim();
    if (t && !out.some((l) => l.text === t)) out.push({ text: t, kind });
  };
  add(p.message, 'problem');
  if (showsDetail(p.status)) add(p.detail, 'problem');
  for (const w of p.warnings) add(w, 'caution');
  if (p.sweepDue) add(`Sweep ${p.sweepDue} before moving on.`, 'caution');
  if (p.noBluetooth && !isLinked(p.status)) add('No Bluetooth in this browser.', 'caution');
  return out.slice(0, 2);
}
