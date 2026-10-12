import { LineEditor, endpointNear, joinFit, type EditableLine } from '../src/lib/line-editor.ts';
let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
let selected = '';
const saved: { id: string; pts: [number, number][] }[] = [];
const removed: string[] = [];
const lines: EditableLine[] = [
  { id: 'a', points: [[0, 0], [50, 40], [100, 0]], widths: null },
  { id: 'b', points: [[0, 100], [100, 100]], widths: null },
];
const ed = new LineEditor({
  lines: () => lines, toScreen: (p) => p, toWorld: (x, y) => [x, y],
  selected: () => selected, select: (id) => (selected = id),
  commit: (id, pts) => saved.push({ id, pts }), remove: (id) => removed.push(id), redraw: () => {},
});
ok(!ed.press(200, 200) && selected === '', 'a press on nothing selects nothing');
ok(!ed.press(100, 101) && selected === 'b', 'a press on a line selects it');
ok(ed.handles().length === 2, 'its fit points are the handles');
ok(!ed.press(50, 40) && selected === 'a', 'pressing another line selects that one instead');
ok(ed.press(50, 41) === true, 'pressing a fit point of the selected line picks it up');
ok(ed.move(60, 70) === true, 'moving it');
ok(ed.display()[0].points[1][0] === 60 && lines[0].points[1][0] === 50, 'the live line moves, the stored one waits for the drop');
ok(ed.release() === true && saved.length === 1 && saved[0].pts[1][1] === 70, 'dropping saves the new fit points');
ok(ed.release() === false, 'nothing more to drop');
// adding a fit point by double-click near the span between point 0 and 1
selected = 'a';
ok(ed.addPoint(25, 20) === true && saved.length === 2 && saved[1].pts.length === 4, 'double-click adds a fit point');
ok(ed.addPoint(300, 300) === false, 'far from the line adds nothing');
ok(ed.key({ key: 'Delete' } as KeyboardEvent) === true && removed[0] === 'a' && selected === '', 'Delete removes the selected line');
ok(ed.key({ key: 'Delete' } as KeyboardEvent) === false, 'with nothing selected a key does nothing');
// the eraser end of a pen
removed.length = 0; selected = 'b';
ok(ed.eraseAt(300, 300) === false && removed.length === 0, 'the eraser over nothing deletes nothing');
ok(ed.eraseAt(50, 102) === true && removed[0] === 'b' && selected === '', 'the eraser deletes the line under it (and lets go of it)');
// picking up an unclosed line where it ended
const open: EditableLine[] = [
  { id: 'w', kind: 'wall', points: [[0, 0], [50, 0], [100, 20]], widths: null },
  { id: 'c', kind: 'wall', closed: true, points: [[0, 50], [50, 50], [25, 80]], widths: null },
  { id: 'l', kind: 'ledge', points: [[0, 200], [100, 200]], widths: null },
];
const near = (x: number, y: number, kind = 'wall') => endpointNear(open, kind, (p) => p, x, y);
ok(near(104, 22)?.line.id === 'w' && near(104, 22)?.atStart === false, 'near the end of an open wall');
ok(near(3, 2)?.line.id === 'w' && near(3, 2)?.atStart === true, 'near its start');
ok(near(50, 0) === null, 'the middle of a line is not an end');
ok(near(0, 50) === null, 'a closed line has no ends to pick up');
ok(near(100, 200) === null && near(100, 200, 'ledge')?.line.id === 'l', 'only a line of the tool\'s own kind');
const more = { pts: [[110, 30], [140, 60], [170, 50]] as [number, number][], widths: null };
const j = joinFit(open[0], more, false);
ok(j.pts.length === 5 && j.pts[2][0] === 100 && j.pts[3][0] === 140, 'the new points follow the old end, no gap');
const k = joinFit(open[0], { pts: [[-5, 3], [-30, 10], [-60, 0]] as [number, number][], widths: null }, true);
ok(k.pts[0][0] === -60 && k.pts[k.pts.length - 1][0] === 100 && k.pts.length === 5, 'drawn away from the start goes in front, in line order');
const w = joinFit({ ...open[0], widths: [1, 1, 1] }, { pts: [[110, 30], [140, 60]], widths: [2, 2] }, false);
ok(w.widths!.length === w.pts.length, 'pen weights stay one per point');
console.log(bad ? 'FAILED' : 'line editor checks ok'); process.exit(bad ? 1 : 0);
