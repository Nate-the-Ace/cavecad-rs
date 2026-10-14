// Adding and removing a leader's arrows. A leader's points are [first tip, note, further tips...]: the note is always the
// second point, every other point is where an arrow touches. Pure, so it is checked on its own.

type Pt = number[];

/** The points with a new arrow touching `p`, or null when `p` is no use (it is the note's own spot). */
export function withArrow<T extends Pt>(points: T[], p: T): T[] | null {
  if (points.length < 2) return null;
  if (points.some((q) => Math.hypot(q[0] - p[0], q[1] - p[1]) < 1e-9)) return null;
  return [...points, p];
}

/** The points without the arrow at `index`, or null when that is the note (index 1) or the leader's only arrow. */
export function withoutArrow<T extends Pt>(points: T[], index: number): T[] | null {
  if (index === 1 || index < 0 || index >= points.length || points.length <= 2) return null;
  // the first point is always a tip, so dropping it brings the next tip up in its place
  if (index === 0) return [points[2], points[1], ...points.slice(3)];
  return points.filter((_, i) => i !== index);
}

/** Which arrow of a leader at screen `pts` a press at (`x`, `y`) is on: its index, or -1. The note (index 1) is never one. */
export function arrowAt(pts: [number, number][], x: number, y: number, reach: number): number {
  let best = -1;
  let bd = reach;
  pts.forEach((q, i) => {
    if (i === 1) return;
    const d = Math.hypot(q[0] - x, q[1] - y);
    if (d <= bd) {
      bd = d;
      best = i;
    }
  });
  return best;
}

// Fastening an arrow to another drawn thing. A tip that sits exactly on a vertex of another line (or on a symbol's insertion
// point) is attached to it: the leader's style remembers `index=id#vertex`, and every view draws that tip where the vertex is now,
// so the arrow follows when the other thing is moved. Nothing else is stored.

interface Thing {
  id?: string;
  kind?: string;
  points: number[][];
  style?: string;
}

/** True for a thing a leader may be fastened to: a line (any vertex) or a symbol (its insertion point), not another leader or words. */
function fastenable(l: Thing, selfId: string): boolean {
  return !!l.id && l.id !== selfId && l.kind !== 'callout' && l.kind !== 'text' && l.points.length > 0;
}

/** The vertices a leader tip may snap to, as [thing id, vertex index, point]. */
function vertices(lines: Thing[], selfId: string): [string, number, number[]][] {
  const out: [string, number, number[]][] = [];
  for (const l of lines) {
    if (!fastenable(l, selfId)) continue;
    const stamp = !!l.kind && (l.kind.startsWith('sym-'));
    l.points.forEach((p, n) => {
      if (!stamp || n === 0) out.push([l.id!, n, p]);
    });
  }
  return out;
}

/** The vertex nearest screen (`x`, `y`) within `reach` pixels (other things only), or null. */
export function nearestVertex<T extends number[]>(lines: { id?: string; kind?: string; points: T[] }[], selfId: string, toScreen: (p: T) => [number, number], x: number, y: number, reach: number): T | null {
  let best: T | null = null;
  let bd = reach;
  for (const [, , p] of vertices(lines, selfId)) {
    const s = toScreen(p as T);
    const d = Math.hypot(s[0] - x, s[1] - y);
    if (d <= bd) {
      bd = d;
      best = p as T;
    }
  }
  return best;
}

/** Reads stored `index=id#vertex` pairs. */
export function parseAttach(text: string | undefined): Map<number, [string, number]> {
  const m = new Map<number, [string, number]>();
  for (const pair of (text ?? '').split(',')) {
    const [i, rest = ''] = pair.split('=');
    const [id, n] = rest.split('#');
    const index = Number(i);
    const vertex = Number(n);
    if (id && Number.isInteger(index) && Number.isInteger(vertex)) m.set(index, [id, vertex]);
  }
  return m;
}

/** Which tips of a leader (at `points`) sit exactly on a vertex of another thing, written as stored. Empty when none. */
export function attachOf(points: number[][], lines: Thing[], selfId: string): string {
  const verts = vertices(lines, selfId);
  const pairs: string[] = [];
  points.forEach((p, i) => {
    if (i === 1) return;
    const hit = verts.find(([, , q]) => Math.abs(q[0] - p[0]) < 1e-9 && Math.abs(q[1] - p[1]) < 1e-9);
    if (hit) pairs.push(`${i}=${hit[0]}#${hit[1]}`);
  });
  return pairs.join(',');
}

/** The lines with every attached leader's tips moved to where the things they are fastened to are now. */
export function resolveLeaders<T extends Thing>(lines: T[]): T[] {
  if (!lines.some((l) => l.kind === 'callout' && (l.style ?? '').split(';')[9])) return lines;
  const byId = new Map(lines.filter((l) => l.id).map((l) => [l.id!, l]));
  return lines.map((l) => {
    if (l.kind !== 'callout') return l;
    const attach = parseAttach((l.style ?? '').split(';')[9]);
    if (!attach.size) return l;
    const points = l.points.map((p, i) => {
      const [id, n] = attach.get(i) ?? ['', 0];
      const to = id && i !== 1 ? byId.get(id)?.points[n] : undefined;
      return to ? (p.length > to.length ? [...to, ...p.slice(to.length)] : to.slice(0, p.length)) : p;
    });
    return { ...l, points };
  });
}
