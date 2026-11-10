import { eraseLines, type EraseLine, type Pt } from '../src/lib/erase.ts';

const ok = (c: boolean, what: string) => {
  if (!c) throw new Error(what);
};
const near = (a: number, b: number, what: string, tol = 1e-6) => {
  if (Math.abs(a - b) > tol) throw new Error(`${what}: got ${a}, wanted ${b}`);
};
const len = (p: Pt[]) => p.slice(1).reduce((s, q, i) => s + Math.hypot(q[0] - p[i][0], q[1] - p[i][1]), 0);

// A straight line crossed in the middle: longer side keeps the id, shorter side is an add, ends sit on the circle.
{
  const line: EraseLine = { id: 'a', kind: 'wall', points: [[0, 0], [10, 0]] };
  const before = JSON.stringify(line);
  const r = eraseLines([line], [[4, -3], [4, 3]], 1);
  ok(JSON.stringify(line) === before, 'input untouched');
  ok(r.remove.length === 0 && r.edits.length === 1 && r.adds.length === 1, 'one edit, one add');
  ok(r.edits[0].id === 'a', 'edit keeps id');
  near(r.edits[0].points[0][0], 5, 'long side starts at circle edge');
  near(r.edits[0].points[r.edits[0].points.length - 1][0], 10, 'long side ends at line end');
  ok(r.adds[0].from === 'a', 'add copies from');
  near(r.adds[0].points[0][0], 0, 'short side start');
  near(r.adds[0].points[r.adds[0].points.length - 1][0], 3, 'short side ends at circle edge');
  ok(r.edits[0].widths === null && r.adds[0].widths === null, 'no widths stays null');
}

// Wholly inside the radius: removed. Untouched: absent.
{
  const r = eraseLines(
    [
      { id: 'in', kind: 'wall', points: [[0, 0], [0.5, 0]] },
      { id: 'far', kind: 'wall', points: [[20, 20], [30, 20]] },
    ],
    [[0, 0]],
    1,
  );
  ok(r.remove.length === 1 && r.remove[0] === 'in', 'inside removed');
  ok(r.edits.length === 0 && r.adds.length === 0, 'untouched line absent');
}

// A closed square cut on one side: one open line, no add.
{
  const sq: EraseLine = { id: 'sq', kind: 'wall', closed: true, points: [[0, 0], [10, 0], [10, 10], [0, 10]] };
  const r = eraseLines([sq], [[5, -2], [5, 2]], 1);
  ok(r.remove.length === 0 && r.adds.length === 0 && r.edits.length === 1, 'closed square: one edit');
  const p = r.edits[0].points;
  near(len(p), 40 - 2 * Math.sqrt(1) , 'rest of the way round', 1e-6);
  near(p[0][0], 6, 'starts just after the cut');
  near(p[0][1], 0, 'on the cut edge');
  near(p[p.length - 1][0], 4, 'ends just before the cut');
  // Ring untouched when the path is far away.
  ok(eraseLines([sq], [[50, 50]], 1).edits.length === 0, 'closed untouched absent');
}

// Fills: outline entered, or path wholly inside.
{
  const fill: EraseLine = { id: 'f', kind: 'fill-sand', closed: true, points: [[0, 0], [10, 0], [10, 10], [0, 10]] };
  ok(eraseLines([fill], [[-3, 5], [0.5, 5]], 1).remove[0] === 'f', 'path reaching outline');
  ok(eraseLines([fill], [[-3, 5], [-0.5, 5]], 1).remove[0] === 'f', 'path within radius of outline');
  ok(eraseLines([fill], [[5, 5]], 1).remove[0] === 'f', 'path wholly inside');
  ok(eraseLines([fill], [[-5, 5]], 1).remove.length === 0, 'path outside, away');
}

// Stamps.
{
  const stamps: EraseLine[] = [
    { id: 's', kind: 'sym-stalactite', points: [[0, 0], [0, 1]] },
    { id: 't', kind: 'text', points: [[5, 5]] },
    { id: 'c', kind: 'callout', points: [[8, 8], [9, 9]] },
  ];
  const r = eraseLines(stamps, [[0.5, 0.5]], 1);
  ok(r.remove.length === 1 && r.remove[0] === 's', 'symbol hit');
  ok(eraseLines(stamps, [[5.5, 5]], 1).remove[0] === 't', 'text hit');
  ok(eraseLines(stamps, [[9.2, 9]], 1).remove[0] === 'c', 'callout hit by second point');
  ok(eraseLines(stamps, [[100, 100]], 1).remove.length === 0, 'stamps untouched');
}

// Widths interpolate with the points.
{
  const line: EraseLine = { id: 'w', kind: 'wall', points: [[0, 0], [10, 0]], widths: [1, 3] };
  const r = eraseLines([line], [[7, -3], [7, 3]], 1);
  const e = r.edits[0];
  ok(e.widths !== null && e.widths.length === e.points.length, 'widths per point');
  near(e.widths![0], 1, 'width of kept start');
  near(e.widths![e.widths!.length - 1], 1 + 2 * 0.6, 'width at cut');
  near(r.adds[0].widths![0], 2.6, 'width interpolated at the add start');
}

// Short slivers are dropped.
{
  const r = eraseLines([{ id: 'x', kind: 'wall', points: [[0, 0], [10, 0]] }], [[2.4, -3], [2.4, 3]], 2);
  ok(r.adds.length === 0, 'sliver under radius/4 dropped');
}

console.log('erase ok');
