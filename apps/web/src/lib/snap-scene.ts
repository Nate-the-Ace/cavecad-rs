// Describing what is on screen to `cave-snap`: the drawn lines, stamps and stations of a view as the things the engine
// snaps to. Each curve is given in its own drawing plane; a stamp (symbol, text, leader) offers its insertion point; a station
// is a point. Hidden things and the thing being edited are left out by the caller.

import type { SnapFrame, SnapThing, V3 } from './snap-types';

/** A drawn line as a view holds it. Points are `[x, y]` or `[x, y, z]`. */
export interface SnapLine {
  id: string;
  kind: string;
  points: number[][];
  closed?: boolean;
  text?: string;
}

export interface SceneOptions {
  /** True for a kind whose first point is an insertion point (a symbol, a text, a leader) rather than a curve. */
  stamp: (kind: string) => boolean;
  /** The plane a line lies in. The map's lines lie in the plan at the height of their first point. */
  frameOf: (line: SnapLine) => SnapFrame;
}

/** The plan frame at height `z`. */
const plan = (z: number): SnapFrame => ({ origin: [0, 0, z], u: [1, 0, 0], v: [0, 1, 0] });

/** The things the engine snaps to, made from a view's lines and stations. */
export function sceneThings(lines: SnapLine[], stations: { id: string; label: string; at: number[] }[], o: Partial<SceneOptions> & Pick<SceneOptions, 'stamp'>): SnapThing[] {
  const frameOf = o.frameOf ?? ((l: SnapLine) => plan(l.points[0]?.[2] ?? 0));
  const out: SnapThing[] = [];
  for (const l of lines) {
    if (l.points.length === 0) continue;
    const frame = frameOf(l);
    if (o.stamp(l.kind)) {
      // A stamp's other points are its size and turn handles (or, for a leader, its tips): only the first is a place.
      const at: V3 = [l.points[0][0], l.points[0][1], l.points[0][2] ?? frame.origin[2]];
      out.push({ id: l.id, kind: l.kind === 'text' ? 'text' : l.kind === 'callout' ? 'leader' : 'symbol', frame, geom: { type: 'point', at }, label: l.text ?? '' });
      continue;
    }
    if (l.points.length < 2) continue;
    out.push({ id: l.id, kind: 'line', frame, geom: { type: 'spline', pts: l.points.map((p) => [p[0], p[1]] as [number, number]), closed: !!l.closed }, label: l.kind });
  }
  for (const s of stations) out.push({ id: s.id, kind: 'station', frame: plan(s.at[2]), geom: { type: 'point', at: [s.at[0], s.at[1], s.at[2]] }, label: s.label });
  return out;
}
