// A placed symbol's size and turn, read from and written to its two points: where it sits, and the point whose
// distance is its size (radius, in the view's metres) and whose direction is its up. Used by the selected
// symbol's tab (Size and Rotation), so a symbol can be resized and turned exactly, not only by dragging.
import { engine } from './engine';
import { cave } from './cave.svelte';

type Pt = [number, number];

/** Size (metres) and rotation (degrees clockwise from the view's up, 0–360) of a symbol's two points. */
export function shapeOf(points: Pt[]): { size: number; rotation: number } | null {
  if (points.length < 2) return null;
  const [a, b] = points;
  const dx = b[0] - a[0];
  const dy = b[1] - a[1];
  const size = Math.hypot(dx, dy);
  if (!(size > 0)) return null;
  const rotation = ((Math.atan2(dx, dy) * 180) / Math.PI + 360) % 360;
  return { size, rotation };
}

/** The two points of a symbol with that size and rotation, keeping where it sits. */
export function pointsFor(anchor: Pt, size: number, rotation: number): Pt[] {
  const r = (rotation * Math.PI) / 180;
  return [anchor, [anchor[0] + size * Math.sin(r), anchor[1] + size * Math.cos(r)]];
}

/** A symbol's points as its view places them (map: plan metres; profile: band metres; section: plane metres). */
export function placedPoints(id: string, view: string): Pt[] | null {
  const pick = (list: { id: string; points: number[][] }[] | undefined) => {
    const p = list?.find((s) => s.id === id)?.points;
    return p ? p.map((q) => [q[0], q[1]] as Pt) : null;
  };
  if (view === 'map') return pick(engine.mapLines(cave.adjusted));
  if (view === 'profile') return pick(engine.profileBand(cave.run, cave.adjusted)?.sketches);
  if (view === 'section') return pick(engine.sectionAt(cave.adjusted, cave.station, false)?.sketches);
  return null;
}

/** Sets a symbol's size and/or rotation (whichever is given), keeping where it sits. Returns `ok` or why not. */
export function setShape(id: string, view: string, change: { size?: number; rotation?: number }): string {
  const pts = placedPoints(id, view);
  const now = pts && shapeOf(pts);
  if (!pts || !now) return 'error: that symbol cannot be found in this view';
  const size = change.size ?? now.size;
  const rotation = change.rotation ?? now.rotation;
  if (!(size > 0) || !Number.isFinite(rotation)) return 'error: not a size or a rotation';
  return engine.editLine(id, pointsFor(pts[0], size, rotation), null, cave.adjusted);
}
