// Pure helpers for the LiDAR rooms read from CaveCAD Capture on the same phone (capture-shots.svelte.ts):
// a stable colour per shot, the thinning cell that keeps every room inside one point budget, the cache key
// of a room's points, and where a room is drawn. No engine, no Tauri: checked by tests/captureshots.check.ts.

/** FNV-1a 32 of a text. */
export function fnv32(s: string): number {
  let h = 0x811c9dc5;
  for (let i = 0; i < s.length; i++) {
    h ^= s.charCodeAt(i);
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h >>> 0;
}

function hslToRgb(h: number, s: number, l: number): number {
  const k = (n: number) => (n + h * 12) % 12;
  const a = s * Math.min(l, 1 - l);
  const f = (n: number) => l - a * Math.max(-1, Math.min(k(n) - 3, Math.min(9 - k(n), 1)));
  const c = (n: number) => Math.round(f(n) * 255);
  return (c(0) << 16) | (c(8) << 8) | c(4);
}

/** A room's colour (0xRRGGBB): its shot id hashed, the hue stepped by the golden angle so neighbours differ. */
export function colorFor(shotId: string): number {
  const hue = (fnv32(shotId) * 0.618033988749895) % 1;
  return hslToRgb(hue, 0.65, 0.55);
}

/** Total points drawn for every room together, at most (about 18 MB of float32). */
export const POINT_BUDGET = 1_500_000;

/** The thinning cell (metres) for rooms of `totalVertices` mesh vertices together: 8 cm, doubled until the
 *  estimate (a LiDAR mesh has roughly one vertex per 2 cm, so a cell keeps about (0.02 / cell)² of them) fits. */
export function cellFor(totalVertices: number, budget = POINT_BUDGET): number {
  let cell = 0.08;
  const estimate = (c: number) => Math.min(totalVertices, totalVertices * (0.02 / c) ** 2);
  while (estimate(cell) > budget && cell < 5) cell *= 2;
  return cell;
}

/** What a room's fetched points depend on: the file (and when it changed), its placement, and the cell. */
export function pointsKey(row: { file: string; modifiedMs: number; rigid: number[] | null; anchor: number[] | null }, cell: number): string {
  const r = (row.rigid ?? []).map((v, i) => (i === 0 ? v.toFixed(6) : v.toFixed(3))).join(',');
  const a = (row.anchor ?? []).map((v) => v.toFixed(3)).join(',');
  return `${row.file}|${row.modifiedMs}|${r}|${a}|${cell}`;
}

/** Where a room is drawn: on its station as the view shows it (so it follows Adjusted and survey edits), else at its anchor. */
export function roomAt(row: { stationId: string | null; anchor: [number, number, number] | null }, shown: Map<string, [number, number, number]>): [number, number, number] | null {
  return (row.stationId ? shown.get(row.stationId) : undefined) ?? row.anchor;
}

/** Counts by status, for the Show menu and the log line. */
export function counts(rows: { status: string }[]): { placed: number; waiting: number; gone: number; unreadable: number } {
  const c = { placed: 0, waiting: 0, gone: 0, unreadable: 0 };
  for (const r of rows) if (r.status in c) c[r.status as keyof typeof c]++;
  return c;
}

/** What a shot's slices depend on: the file (and when it changed) and its placement (no thinning cell). */
export function placeKey(row: { file: string; modifiedMs: number; rigid: number[] | null; anchor: number[] | null }): string {
  return pointsKey(row, 0);
}

/** Profile walls `[s, up]` from a shot's station put on the band: at the station's band `s` and `z`, as the band places its stations. */
export function onBand(lines: [number, number][][], at: { s: number; z: number }): [number, number][][] {
  return lines.map((l) => l.map((p) => [at.s + p[0], at.z + p[1]] as [number, number]));
}

/** The colour of a LiDAR cross section (rooms merge there, so one colour, not the rooms' own): a magenta no survey type uses. */
export const LIDAR_SECTION_COLOR = 0xd63384;

/** A 0xRRGGBB colour as CSS. */
export function cssColor(c: number): string {
  return `#${c.toString(16).padStart(6, '0')}`;
}

/** Where the small "LiDAR" tag goes: the highest point of the longest line (out of the way of the floor), or null. */
export function tagSpot(lines: [number, number][][]): [number, number] | null {
  let best: [number, number][] | null = null;
  let most = -1;
  for (const l of lines) {
    let len = 0;
    for (let i = 1; i < l.length; i++) len += Math.hypot(l[i][0] - l[i - 1][0], l[i][1] - l[i - 1][1]);
    if (len > most) {
      most = len;
      best = l;
    }
  }
  if (!best || !best.length) return null;
  return best.reduce((a, p) => (p[1] > a[1] ? p : a), best[0]);
}
