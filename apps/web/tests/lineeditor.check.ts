import { LineEditor, endpointNear, joinFit, topLineAt, type EditableLine } from '../src/lib/line-editor.ts';
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
// fitting a line again at another detail
const dense: EditableLine = { id: 'd', kind: 'wall', points: Array.from({ length: 30 }, (_, i) => [i * 4, Math.sin(i / 4) * 30] as [number, number]), widths: null };
lines.push(dense);
saved.length = 0;
ok(ed.refitLine('d', 1) === true && saved.length === 1 && saved[0].id === 'd' && saved[0].pts.length < 30, 'a loose re-fit has fewer points and is saved as one edit');
ok(ed.refitLine('nothing', 5) === false, 'no such line');
lines.push({ id: 'sym', kind: 'sym-pit', points: [[0, 0], [0, 10]], widths: null });
ok(ed.refitLine('sym', 5) === false, 'a symbol has no curve to re-fit');
lines.pop(); lines.pop();
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
// a symbol: its insertion point carries the whole symbol; its other handle sets the size and turn
{
  const sy: EditableLine = { id: 'pit1', kind: 'sym-pit', points: [[200, 200], [200, 230]], widths: null };
  lines.push(sy);
  saved.length = 0;
  selected = 'pit1';
  ok(ed.press(200, 200) === true, 'the insertion point of a symbol is picked up');
  ed.move(260, 210);
  ok(ed.release() === true && saved[0].pts[0][0] === 260 && saved[0].pts[0][1] === 210 && saved[0].pts[1][0] === 260 && saved[0].pts[1][1] === 240, 'moving the insertion point moves the whole symbol: size and turn are unchanged');
  sy.points = [[200, 200], [200, 230]];
  saved.length = 0;
  ok(ed.press(200, 230) === true, 'the size and turn handle is picked up');
  ed.move(240, 200);
  ed.release();
  ok(saved[0].pts[0][0] === 200 && saved[0].pts[0][1] === 200 && saved[0].pts[1][0] === 240, 'moving the other handle changes only the size and turn');
  lines.pop();
}
// an area is selected by its hatch (anywhere inside it) as well as by its outline; a line over it still wins
{
  const id = (p: [number, number]) => p;
  const fill: EditableLine = { id: 'f', kind: 'fill-sand', closed: true, points: [[0, 0], [200, 0], [200, 200], [0, 200]], widths: null };
  const wall: EditableLine = { id: 'w', kind: 'wall', points: [[20, 100], [180, 100]], widths: null };
  const lines = [fill, wall];
  ok(topLineAt(lines, id, 100, 150, 7) === 0, 'a click inside the area, away from its edge, selects the fill');
  ok(topLineAt(lines, id, 100, 101, 7) === 1, 'a click on a wall that lies over the fill selects the wall');
  ok(topLineAt(lines, id, 1, 100, 7) === 0, 'a click on the fill\'s own edge selects it');
  ok(topLineAt(lines, id, 400, 400, 7) === -1, 'a click outside everything selects nothing');
  const plain: EditableLine = { id: 'p', kind: 'wall', closed: true, points: [[0, 0], [200, 0], [200, 200], [0, 200]], widths: null };
  ok(topLineAt([plain], id, 100, 150, 7) === -1, 'a closed wall (not an area fill) is still picked by its line only, not by its inside');
  const over = [fill, { id: 'g', kind: 'fill-clay', closed: true, points: [[50, 50], [150, 50], [150, 150], [50, 150]], widths: null } as EditableLine];
  ok(topLineAt(over, id, 100, 100, 7) === 1, 'of two areas over each other the topmost one is picked');
}
// a shaped line is picked by any part of its marks, not only by the line
{
  const id = (p: [number, number]) => p;
  const ledge: EditableLine = { id: 'l', kind: 'ledge', points: [[0, 100], [200, 100]], widths: null };
  // ticks hanging 12 px below the line every 20 px
  const ticks = () => Array.from({ length: 10 }, (_, i): [number, number][] => [[10 + i * 20, 100], [10 + i * 20, 112]]);
  const ornament = () => ticks();
  ok(topLineAt([ledge], id, 70, 111, 7) === -1, 'without the marks, a click on a tick misses the line');
  ok(topLineAt([ledge], id, 70, 111, 7, undefined, ornament) === 0, 'a click on a tick end picks the line');
  ok(topLineAt([ledge], id, 74, 108, 7, undefined, ornament) === 0, 'and so does one beside a tick');
  ok(topLineAt([ledge], id, 70, 160, 7, undefined, ornament) === -1, 'but not one far from every mark');
  ok(topLineAt([ledge], id, 100, 101, 7, undefined, ornament) === 0, 'the line itself still works');
}
// a double click on a symbol or a leader finds its words; on a plain line it does not
lines.push({ id: 's', points: [[200, 200], [200, 180]], widths: null, kind: 'sym-pit', text: '30' }, { id: 'l', points: [[300, 300], [380, 280]], widths: null, kind: 'callout', text: 'Squeeze' });
const sw = ed.wordsAt(201, 200);
ok(!!sw && sw.id === 's' && sw.text === '30' && sw.x > 200, `a symbol's label is found, got ${JSON.stringify(sw)}`);
const n = ed.wordsAt(301, 300); // (the note's own box needs the engine's layout; here the tip is what answers)
ok(!!n && n.id === 'l' && n.text === 'Squeeze' && Math.abs(n.x - 380) < 1, `a leader's note is found, got ${JSON.stringify(n)}`);
ok(ed.wordsAt(100, 101) === null, 'a plain line has no words to edit');
console.log(bad ? 'FAILED' : 'line editor checks ok'); process.exit(bad ? 1 : 0);
