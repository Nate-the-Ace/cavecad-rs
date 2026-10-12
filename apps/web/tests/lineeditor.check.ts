import { LineEditor, type EditableLine } from '../src/lib/line-editor.ts';
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
console.log(bad ? 'FAILED' : 'line editor checks ok'); process.exit(bad ? 1 : 0);
