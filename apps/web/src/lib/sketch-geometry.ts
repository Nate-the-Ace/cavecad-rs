// The geometry behind the sketchpad: the survey turned into the three flat views a cave sketch is made on
// (plan, profile, cross section), and strokes that stay attached to the survey. Pure and tested
// (tests/sketch.check.ts); the pad that draws it is `panels/SketchPad.svelte`.
//
// The views are the ordinary ones: PLAN looks straight down (x east, y north); PROFILE is the extended
// elevation, the main chain unrolled onto one horizontal axis (distance along the passage, elevation);
// SECTION looks along the passage at one station (across to the right, up). Walls come from the passage
// dimensions (L R U D). A stroke is stored as offsets from the nearest station in its view, never as screen
// or world coordinates, so it moves with the survey if a station shifts.

export type Pt = [number, number];

/** One shot as the notebook holds it. Tape in metres, angles in degrees; backsights as read (compass about 180 off, clino flipped). */
export interface SurveyShot {
  id: string;
  from: string;
  /** `null` for a splay. */
  to: string | null;
  distance: number | null;
  az: number | null;
  inc: number | null;
  azBack?: number | null;
  incBack?: number | null;
}

export interface Dims {
  left: number | null;
  right: number | null;
  up: number | null;
  down: number | null;
}
export type DimsByStation = Record<string, Dims>;

export interface Station {
  label: string;
  /** East, north, up in metres from the first station. */
  x: number;
  y: number;
  z: number;
}

export interface Layout {
  stations: Record<string, Station>;
  legs: { id: string; from: string; to: string }[];
  splays: { id: string; from: string; end: [number, number, number] }[];
  /** The longest run of stations from the first, the spine of the profile. */
  chain: string[];
}

const rad = (d: number) => (d * Math.PI) / 180;
const deg = (r: number) => (r * 180) / Math.PI;
const mod360 = (a: number) => ((a % 360) + 360) % 360;

/** A shot as a vector (east, north, up), combining foresight and backsight when both are there. `null` when it cannot be placed. */
export function shotVector(s: Pick<SurveyShot, 'distance' | 'az' | 'inc' | 'azBack' | 'incBack'>): [number, number, number] | null {
  if (s.distance === null || s.distance <= 0 || s.inc === null) return null;
  const inc = s.incBack !== null && s.incBack !== undefined ? (s.inc - s.incBack) / 2 : s.inc;
  let az = s.az;
  if (az !== null && s.azBack !== null && s.azBack !== undefined) {
    // Circular mean of the foresight and the backsight turned round.
    const a = rad(az);
    const b = rad(s.azBack + 180);
    az = mod360(deg(Math.atan2(Math.sin(a) + Math.sin(b), Math.cos(a) + Math.cos(b))));
  }
  if (az === null) {
    if (Math.abs(inc) < 85) return null; // only a pitch has no bearing
    az = 0;
  }
  const h = Math.cos(rad(inc)) * s.distance;
  return [h * Math.sin(rad(az)), h * Math.cos(rad(az)), Math.sin(rad(inc)) * s.distance];
}

/** Places every station reachable from the first, leg by leg. Loops are drawn as they close (no adjustment here). */
export function layout(shots: SurveyShot[]): Layout {
  const stations: Record<string, Station> = {};
  const at = (label: string, x: number, y: number, z: number) => (stations[label] = { label, x, y, z });
  const legs = shots.filter((s) => s.to !== null).map((s) => ({ s, v: shotVector(s) }));
  const first = legs[0]?.s.from ?? shots[0]?.from;
  if (!first) return { stations, legs: [], splays: [], chain: [] };
  at(first, 0, 0, 0);
  const pending = legs.filter((l) => l.v !== null);
  let moved = true;
  while (moved && pending.length > 0) {
    moved = false;
    for (let i = pending.length - 1; i >= 0; i--) {
      const { s, v } = pending[i];
      const a = stations[s.from];
      const b = stations[s.to!];
      if (a && !b) at(s.to!, a.x + v![0], a.y + v![1], a.z + v![2]);
      else if (b && !a) at(s.from, b.x - v![0], b.y - v![1], b.z - v![2]);
      else if (!a && !b) continue;
      pending.splice(i, 1);
      moved = true;
    }
  }
  const placed = legs.filter((l) => stations[l.s.from] && stations[l.s.to!]).map((l) => ({ id: l.s.id, from: l.s.from, to: l.s.to! }));
  const splays: Layout['splays'] = [];
  for (const s of shots.filter((x) => x.to === null)) {
    const v = shotVector(s);
    const a = stations[s.from];
    if (v && a) splays.push({ id: s.id, from: s.from, end: [a.x + v[0], a.y + v[1], a.z + v[2]] });
  }
  return { stations, legs: placed, splays, chain: mainChain(first, placed, stations) };
}

/** The longest run of legs from `start` (by length), following the survey's branches. */
function mainChain(start: string, legs: { from: string; to: string }[], st: Record<string, Station>): string[] {
  const next = new Map<string, string[]>();
  for (const l of legs) {
    (next.get(l.from) ?? next.set(l.from, []).get(l.from)!).push(l.to);
    (next.get(l.to) ?? next.set(l.to, []).get(l.to)!).push(l.from);
  }
  const len = (a: string, b: string) => Math.hypot(st[a].x - st[b].x, st[a].y - st[b].y, st[a].z - st[b].z);
  let best: { path: string[]; length: number } = { path: [start], length: 0 };
  const walk = (at: string, path: string[], length: number, seen: Set<string>) => {
    if (length > best.length) best = { path: [...path], length };
    for (const n of next.get(at) ?? []) {
      if (seen.has(n)) continue;
      seen.add(n);
      path.push(n);
      walk(n, path, length + len(at, n), seen);
      path.pop();
      seen.delete(n);
    }
  };
  walk(start, [start], 0, new Set([start]));
  return best.path;
}

/** Heading (unit east, north) of the passage at chain position `i`: the bisector of the legs either side. */
function heading(l: Layout, i: number): Pt {
  const c = l.chain;
  const a = l.stations[c[Math.max(0, i - 1)]];
  const b = l.stations[c[Math.min(c.length - 1, i + 1)]];
  const dx = b.x - a.x;
  const dy = b.y - a.y;
  const n = Math.hypot(dx, dy);
  return n < 1e-9 ? [0, 1] : [dx / n, dy / n];
}

/** What a view draws, in that view's own metres (y up). */
export interface PlanView {
  stations: { label: string; p: Pt }[];
  legs: [Pt, Pt][];
  splays: [Pt, Pt][];
  /** Wall lines (left and right of travel), each a run of points. */
  walls: Pt[][];
  /** The passage between the walls, as polygons. */
  passage: Pt[][];
}

/** Splits `items` into runs of consecutive entries where `f` gives a value. */
function runs<T, R>(items: T[], f: (t: T, i: number) => R | null): R[][] {
  const out: R[][] = [];
  let cur: R[] = [];
  items.forEach((t, i) => {
    const v = f(t, i);
    if (v === null) {
      if (cur.length) out.push(cur);
      cur = [];
    } else cur.push(v);
  });
  if (cur.length) out.push(cur);
  return out;
}

export function planView(l: Layout, dims: DimsByStation): PlanView {
  const P = (label: string): Pt => [l.stations[label].x, l.stations[label].y];
  const side = (i: number, which: 'left' | 'right'): Pt | null => {
    const d = dims[l.chain[i]]?.[which];
    if (d === null || d === undefined) return null;
    const [hx, hy] = heading(l, i);
    const [nx, ny] = which === 'left' ? [-hy, hx] : [hy, -hx];
    const p = P(l.chain[i]);
    return [p[0] + nx * d, p[1] + ny * d];
  };
  const lefts = runs(l.chain, (_, i) => side(i, 'left'));
  const rights = runs(l.chain, (_, i) => side(i, 'right'));
  // A passage polygon where a left run and a right run cover the same stations.
  const passage: Pt[][] = [];
  let run: { left: Pt; right: Pt }[] = [];
  const flush = () => {
    if (run.length > 1) passage.push([...run.map((r) => r.left), ...run.map((r) => r.right).reverse()]);
    run = [];
  };
  l.chain.forEach((_, i) => {
    const a = side(i, 'left');
    const b = side(i, 'right');
    if (a && b) run.push({ left: a, right: b });
    else flush();
  });
  flush();
  return {
    stations: Object.values(l.stations).map((s) => ({ label: s.label, p: [s.x, s.y] as Pt })),
    legs: l.legs.map((g) => [P(g.from), P(g.to)] as [Pt, Pt]),
    splays: l.splays.map((s) => [P(s.from), [s.end[0], s.end[1]] as Pt] as [Pt, Pt]),
    walls: [...lefts, ...rights].filter((r) => r.length > 1),
    passage,
  };
}

export interface ProfileView {
  /** Stations on the main chain: x is distance along it, y elevation. */
  stations: { label: string; p: Pt }[];
  legs: [Pt, Pt][];
  /** Floor and ceiling lines. */
  walls: Pt[][];
  passage: Pt[][];
}

export function profileView(l: Layout, dims: DimsByStation): ProfileView {
  let s = 0;
  const along: Pt[] = l.chain.map((label, i) => {
    if (i > 0) {
      const a = l.stations[l.chain[i - 1]];
      const b = l.stations[label];
      s += Math.hypot(b.x - a.x, b.y - a.y);
    }
    return [s, l.stations[label].z];
  });
  const at = (i: number, which: 'up' | 'down'): Pt | null => {
    const d = dims[l.chain[i]]?.[which];
    return d === null || d === undefined ? null : [along[i][0], along[i][1] + (which === 'up' ? d : -d)];
  };
  const ceilings = runs(l.chain, (_, i) => at(i, 'up'));
  const floors = runs(l.chain, (_, i) => at(i, 'down'));
  const passage: Pt[][] = [];
  let run: { up: Pt; down: Pt }[] = [];
  const flush = () => {
    if (run.length > 1) passage.push([...run.map((r) => r.up), ...run.map((r) => r.down).reverse()]);
    run = [];
  };
  l.chain.forEach((_, i) => {
    const u = at(i, 'up');
    const d = at(i, 'down');
    if (u && d) run.push({ up: u, down: d });
    else flush();
  });
  flush();
  return {
    stations: l.chain.map((label, i) => ({ label, p: along[i] })),
    legs: along.slice(1).map((p, i) => [along[i], p] as [Pt, Pt]),
    walls: [...ceilings, ...floors].filter((r) => r.length > 1),
    passage,
  };
}

export interface SectionView {
  station: string;
  /** The passage outline from the dimensions: left, up, right, down. `null` with no dimensions at all. */
  outline: Pt[] | null;
  /** The ends of the splays taken at this station, across and up. */
  splays: Pt[];
}

export function sectionView(l: Layout, dims: DimsByStation, station: string): SectionView | null {
  const st = l.stations[station];
  if (!st) return null;
  const i = l.chain.indexOf(station);
  // The passage direction here: along the chain, or the leg that arrives, or north.
  let h: Pt = [0, 1];
  if (i >= 0) h = heading(l, i);
  else {
    const arr = l.legs.find((g) => g.to === station) ?? l.legs.find((g) => g.from === station);
    if (arr) {
      const a = l.stations[arr.from];
      const b = l.stations[arr.to];
      const n = Math.hypot(b.x - a.x, b.y - a.y);
      if (n > 1e-9) h = [(b.x - a.x) / n, (b.y - a.y) / n];
    }
  }
  const right: Pt = [h[1], -h[0]];
  const d = dims[station];
  const v = (x: number | null | undefined, fallback = 0) => (x === null || x === undefined ? fallback : x);
  const any = !!d && [d.left, d.right, d.up, d.down].some((x) => x !== null);
  return {
    station,
    outline: any ? [[-v(d.left), 0], [0, v(d.up)], [v(d.right), 0], [0, -v(d.down)]] : null,
    splays: l.splays.filter((s) => s.from === station).map((s) => [(s.end[0] - st.x) * right[0] + (s.end[1] - st.y) * right[1], s.end[2] - st.z] as Pt),
  };
}

// ---- strokes that stay attached ----

/** A stroke as stored: its fit points relative to the nearest station of the view it was drawn in. */
export interface Anchored {
  anchor: string;
  /** Offsets from the station, in the view's metres. */
  pts: Pt[];
  /** Pen weight per point (relative, 1 is normal), when there was a pen. */
  widths: number[] | null;
  kind: string;
}

/** Anchors a stroke drawn at world points `pts` (view metres) to the nearest of `stations`. `null` when there are none. */
export function anchorStroke(stations: { label: string; p: Pt }[], pts: Pt[], kind: string, widths: number[] | null = null): Anchored | null {
  if (stations.length === 0 || pts.length === 0) return null;
  const mid = pts[Math.floor(pts.length / 2)];
  let best = stations[0];
  let bd = Infinity;
  for (const s of stations) {
    const d = Math.hypot(s.p[0] - mid[0], s.p[1] - mid[1]);
    if (d < bd) {
      bd = d;
      best = s;
    }
  }
  return { anchor: best.label, pts: pts.map((p) => [p[0] - best.p[0], p[1] - best.p[1]] as Pt), widths, kind };
}

/** Where a stored stroke is now, or `null` when its station is gone. */
export function placeStroke(stations: { label: string; p: Pt }[], a: Anchored): Pt[] | null {
  const s = stations.find((x) => x.label === a.anchor);
  return s ? a.pts.map((p) => [p[0] + s.p[0], p[1] + s.p[1]] as Pt) : null;
}

// ---- fitting a view to a pane ----

export interface Transform {
  /** Pixels per metre. */
  scale: number;
  /** Where the view's origin lands on the pane. */
  tx: number;
  ty: number;
}

export function bounds(points: Pt[]): { x0: number; y0: number; x1: number; y1: number } | null {
  if (points.length === 0) return null;
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  for (const [x, y] of points) {
    x0 = Math.min(x0, x);
    y0 = Math.min(y0, y);
    x1 = Math.max(x1, x);
    y1 = Math.max(y1, y);
  }
  return { x0, y0, x1, y1 };
}

/** A transform that shows `points` whole in a pane of `w` by `h` pixels, true to scale, with `pad` pixels round it and at most `maxScale` pixels per metre. */
export function fitView(points: Pt[], w: number, h: number, pad = 28, maxScale = 80): Transform {
  const b = bounds(points);
  if (!b || w <= 2 * pad || h <= 2 * pad) return { scale: 20, tx: w / 2, ty: h / 2 };
  const spanX = Math.max(b.x1 - b.x0, 1);
  const spanY = Math.max(b.y1 - b.y0, 1);
  const scale = Math.min((w - 2 * pad) / spanX, (h - 2 * pad) / spanY, maxScale);
  const cx = (b.x0 + b.x1) / 2;
  const cy = (b.y0 + b.y1) / 2;
  return { scale, tx: w / 2 - cx * scale, ty: h / 2 + cy * scale };
}

/** World metres to pane pixels (y flips: up on the page is up in the world). */
export const toScreen = (t: Transform, p: Pt): Pt => [t.tx + p[0] * t.scale, t.ty - p[1] * t.scale];
export const toWorld = (t: Transform, p: Pt): Pt => [(p[0] - t.tx) / t.scale, (t.ty - p[1]) / t.scale];
