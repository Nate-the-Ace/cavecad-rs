// Copying drawn lines: what goes on the clipboard, and where a paste puts it. Pure, so it is checked on its own.
// A copy is plain data (kind, points, pen weights, words, style) with no ids, so it can be pasted into any view, into another
// cave, or into another window of the app through the system clipboard.

import { withAttach } from './line-style-text.ts';

/** One copied line. Points are in the space of the view it was copied from. */
export interface ClipLine {
  kind: string;
  points: number[][];
  widths: number[] | null;
  closed: boolean;
  text: string;
  style: string;
}

export interface Clip {
  v: 1;
  /** Which view the lines came from (`map`, `profile` or `section`). */
  view: string;
  lines: ClipLine[];
}

const PREFIX = 'cavecad-clip:';
const MAX_LINES = 2000;

/** What a view gives for a drawn line (more than a copy needs). */
export interface Drawn {
  kind: string;
  points: number[][];
  widths?: number[] | null;
  closed?: boolean;
  text?: string;
  style?: string;
}

/** A copy of drawn lines: plan or view coordinates only (no height), and a leader's fastenings dropped (they name the originals). */
export function clipFrom(view: string, lines: Drawn[]): Clip {
  return {
    v: 1,
    view,
    lines: lines.slice(0, MAX_LINES).map((l) => ({
      kind: l.kind,
      points: l.points.map((p) => [p[0], p[1]]),
      widths: l.widths && l.widths.length === l.points.length ? [...l.widths] : null,
      closed: !!l.closed,
      text: l.text ?? '',
      style: l.kind === 'callout' && l.style ? withAttach(l.style, '') : (l.style ?? ''),
    })),
  };
}

/** The middle of the box round every point of the copy. */
export function centreOf(clip: Clip): [number, number] {
  let [x0, y0, x1, y1] = [Infinity, Infinity, -Infinity, -Infinity];
  for (const l of clip.lines) {
    for (const p of l.points) {
      x0 = Math.min(x0, p[0]);
      y0 = Math.min(y0, p[1]);
      x1 = Math.max(x1, p[0]);
      y1 = Math.max(y1, p[1]);
    }
  }
  return Number.isFinite(x0) ? [(x0 + x1) / 2, (y0 + y1) / 2] : [0, 0];
}

/** The copied lines moved by (`dx`, `dy`). */
export function moved(lines: ClipLine[], dx: number, dy: number): ClipLine[] {
  return lines.map((l) => ({ ...l, points: l.points.map((p) => [p[0] + dx, p[1] + dy]) }));
}

/** The text that goes on the system clipboard. */
export function encodeClip(clip: Clip): string {
  return PREFIX + JSON.stringify(clip);
}

/** A copy read back from clipboard text, or null when the text is not one of ours (or is damaged). */
export function decodeClip(text: string | undefined | null): Clip | null {
  if (!text || !text.startsWith(PREFIX)) return null;
  try {
    const c = JSON.parse(text.slice(PREFIX.length)) as Clip;
    if (c.v !== 1 || !Array.isArray(c.lines) || c.lines.length === 0 || c.lines.length > MAX_LINES) return null;
    for (const l of c.lines) {
      if (typeof l.kind !== 'string' || !/^[\x21-\x7e]{1,40}$/.test(l.kind) || !Array.isArray(l.points) || l.points.length < 2 || l.points.length > 20000) return null;
      if (!l.points.every((p) => Array.isArray(p) && p.length >= 2 && Number.isFinite(p[0]) && Number.isFinite(p[1]))) return null;
      if (l.widths !== null && (!Array.isArray(l.widths) || l.widths.length !== l.points.length || !l.widths.every(Number.isFinite))) return null;
      if (typeof l.text !== 'string' || typeof l.style !== 'string' || typeof l.closed !== 'boolean') return null;
    }
    return { v: 1, view: typeof c.view === 'string' ? c.view : '', lines: c.lines };
  } catch {
    return null;
  }
}
