// Run: npm run test:docktidy
import assert from 'node:assert/strict';
import { columnsFirst, snapEdge, transpose, type Grid } from '../src/lib/dock-tidy.ts';

const leaf = (id: string, size: number) => ({ type: 'leaf' as const, data: { id }, size });

// A 2×2 built row first: one full-width edge between the rows.
const rows: Grid = {
  orientation: 'VERTICAL',
  width: 1000,
  height: 600,
  root: {
    type: 'branch',
    size: 1000,
    data: [
      { type: 'branch', size: 350, data: [leaf('map', 560), leaf('profile', 440)] },
      { type: 'branch', size: 250, data: [leaf('notebook', 560), leaf('scans', 440)] },
    ],
  },
};
const cols = columnsFirst(rows);
assert.ok(cols, 'a lined-up grid is turned');
assert.equal(cols.orientation, 'HORIZONTAL');
assert.equal(cols.root.size, 600);
const [left, right] = (cols.root as { data: { size: number; data: { size: number; data: { id: string } }[] }[] }).data;
assert.deepEqual([left.size, right.size], [560, 440]);
assert.deepEqual(left.data.map((c) => [c.data.id, c.size]), [['map', 350], ['notebook', 250]]);
assert.deepEqual(right.data.map((c) => [c.data.id, c.size]), [['profile', 350], ['scans', 250]]);

// Already column first: left alone by columnsFirst, but it can be turned back into rows exactly.
assert.equal(columnsFirst(cols), null);
assert.deepEqual(transpose(cols), rows);
// Rows whose panels do not line up: no columns to make.
const crooked = structuredClone(rows);
(crooked.root as { data: { data: { size: number }[] }[] }).data[1].data[0].size = 300;
assert.equal(columnsFirst(crooked), null);
// A row with a different number of panels, or a panel split again: left alone.
const uneven = structuredClone(rows);
(uneven.root as { data: { data: unknown[] }[] }).data[1].data.push(leaf('x', 10));
assert.equal(columnsFirst(uneven), null);

// Snapping: near another edge it lands there; far, it stays; on the edge itself, nothing to do.
assert.equal(snapEdge(305, [300, 500]), 300);
assert.equal(snapEdge(330, [300, 500]), 330);
assert.equal(snapEdge(300, [300]), 300);
assert.equal(snapEdge(495, [490, 498]), 498);
console.log('docktidy: ok');
