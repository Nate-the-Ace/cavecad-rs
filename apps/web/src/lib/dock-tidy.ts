// Panels that sit in a grid should slide past one another: dragging the edge between two panels in one column must
// not drag the edges in the column beside it.
//
// dockview keeps the layout as a tree of splits. A 2×2 grid built row first (two full-width rows, each split into a
// left and a right panel) has ONE horizontal edge across the whole width, so moving it moves both columns. Built
// column first, each column has its own edge. `columnsFirst` turns the first shape into the second when the rows'
// panels line up in columns, keeping every panel and its size. Pure, so it is tested in tests/docktidy.check.ts.

/** A node of dockview's saved grid (only what is read here; the rest is carried through untouched). */
export type GridNode = { type: 'branch'; data: GridNode[]; size: number } | { type: 'leaf'; data: unknown; size: number; visible?: boolean };
/** dockview's saved grid. */
export type Grid = { root: GridNode; orientation: 'HORIZONTAL' | 'VERTICAL'; width: number; height: number };

/** How far apart (px) the panel edges of two rows may be and still count as one column edge. */
export const ALIGN_PX = 40;

/**
 * The same layout built column first, or null when it already is, or when its rows' panels do not line up in
 * columns (then there is no column to give its own edge). Only a top-to-bottom stack of rows is turned: rows that
 * each hold the same number of side-by-side panels, none of them split again.
 */
export function columnsFirst(grid: Grid): Grid | null {
  const root = grid.root;
  if (grid.orientation !== 'VERTICAL' || root.type !== 'branch' || root.data.length < 2) return null;
  const rows = root.data;
  if (!rows.every((r) => r.type === 'branch')) return null;
  const n = (rows[0] as Extract<GridNode, { type: 'branch' }>).data.length;
  if (n < 2) return null;
  for (const r of rows) {
    const cells = (r as Extract<GridNode, { type: 'branch' }>).data;
    if (cells.length !== n || !cells.every((c) => c.type === 'leaf')) return null;
  }
  // The column edges of every row must line up with the first row's.
  const edges = (r: GridNode) => {
    let at = 0;
    return (r as Extract<GridNode, { type: 'branch' }>).data.slice(0, -1).map((c) => (at += c.size));
  };
  const first = edges(rows[0]);
  if (!rows.every((r) => edges(r).every((e, i) => Math.abs(e - first[i]) <= ALIGN_PX))) return null;
  const widths = (rows[0] as Extract<GridNode, { type: 'branch' }>).data.map((c) => c.size);
  const height = rows.reduce((s, r) => s + r.size, 0);
  const columns: GridNode[] = widths.map((w, j) => ({
    type: 'branch',
    size: w,
    data: rows.map((r) => ({ ...(r as Extract<GridNode, { type: 'branch' }>).data[j], size: r.size })),
  }));
  return { ...grid, orientation: 'HORIZONTAL', root: { type: 'branch', size: height, data: columns } };
}

/** How close (px) a dragged edge must come to another panel edge to snap to it. */
export const SNAP_PX = 12;

/**
 * Where a dragged edge at `at` should land: the nearest of `others` within `SNAP_PX`, or `at` itself. Edges of the
 * window are among `others`, so a panel can be snapped shut against the top or bottom too.
 */
export function snapEdge(at: number, others: number[]): number {
  let best = at;
  let gap = SNAP_PX + 1;
  for (const o of others) {
    const d = Math.abs(o - at);
    if (d < gap && d > 0.5) {
      gap = d;
      best = o;
    }
  }
  return gap <= SNAP_PX ? best : at;
}
