import { withLabelSpot, hitsStamp } from '../src/lib/symbols.ts';
let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
ok(withLabelSpot('sym-pit', [[0, 0], [0, 2]], '30').length === 3, 'a labelled symbol gets a note point');
ok(withLabelSpot('sym-pit', [[0, 0], [0, 2]], '').length === 2, 'no label, no note point');
ok(withLabelSpot('wall', [[0, 0], [0, 2]], '30').length === 2, 'only symbols');
const [, , note] = withLabelSpot('sym-pit', [[0, 0], [0, 2]], '30');
ok(Math.abs(note[0] - 2.6) < 1e-9 && note[1] === 0, `note sits to the right, got ${note}`);
// the note is picked on its words; pulled far away, the leader to the symbol is picked too
ok(hitsStamp('sym-pit', '30', [0, 0], [0, 10], 14, 0, 4, undefined, [[13, 0]]), 'near note: its words');
ok(hitsStamp('sym-pit', '30', [0, 0], [0, 10], 50, 0, 4, undefined, [[100, 0]]), 'far note: the line to it');
ok(!hitsStamp('sym-pit', '30', [0, 0], [0, 10], 50, 40, 4, undefined, [[100, 0]]), 'away from everything');
import { hitsLabelNote, needsWords } from '../src/lib/symbols.ts';
let bad2 = 0;
const ok2 = (c: boolean, m: string) => { if (!c) { bad2++; console.log('FAIL', m); } };
ok2(hitsLabelNote('sym-pit', '30', [0, 0], [0, 10], [60, 0], 62, 0), 'a press on the note words picks it up');
ok2(!hitsLabelNote('sym-pit', '30', [0, 0], [0, 10], [60, 0], 62, 40), 'not away from it');
ok2(!hitsLabelNote('sym-pit', '', [0, 0], [0, 10], [60, 0], 62, 0), 'no words, no note');
ok2(!needsWords('callout') && needsWords('text'), 'a leader may be empty, text may not');
import { labelTip, placement, withLabelSpots } from '../src/lib/symbols.ts';
import { LineEditor, type EditableLine } from '../src/lib/line-editor.ts';
let bad3 = 0;
const ok3 = (c: boolean, m: string) => { if (!c) { bad3++; console.log('FAIL', m); } };
// a symbol labelled after it was placed (or before labels were notes) gets its note point where the label sat
const spotted = withLabelSpots([
  { kind: 'sym-pit', text: 'P 19', points: [[0, 0], [0, 2]] },
  { kind: 'sym-pit', text: '', points: [[0, 0], [0, 2]] },
  { kind: 'sym-pit', text: 'P 19', points: [[0, 0], [0, 2], [9, 9]] },
  { kind: 'wall', text: 'x', points: [[0, 0], [0, 2]] },
]);
ok3(spotted[0].points.length === 3 && spotted[0].points[2][0] === 2.6, 'a labelled two-point symbol gets its note point');
ok3(spotted[1].points.length === 2 && spotted[3].points.length === 2, 'no label or not a symbol: unchanged');
ok3(spotted[2].points[2][0] === 9, 'a note already placed stays where it was put');
// the leader's arrow lands on the symbol's edge, facing the note
const tip = labelTip(placement([0, 0], [0, 10]), [30, 40]);
ok3(Math.abs(tip[0] - 6) < 1e-9 && Math.abs(tip[1] - 8) < 1e-9, `arrow on the edge, got ${tip}`);
// selected, the label has a grip; dragging it (through the view's snap) saves the note point with the symbol, undoable as one edit
let sel = '';
const commits: { id: string; pts: [number, number][] }[] = [];
const snapped: [number, number][] = [];
const sym: EditableLine[] = [{ id: 's', kind: 'sym-pit', text: 'P 19', points: [[100, 100], [100, 90]], widths: null }];
const ed = new LineEditor({
  lines: () => sym, toScreen: (p) => p, toWorld: (x, y) => [x, y], selected: () => sel, select: (id) => (sel = id),
  commit: (id, pts) => commits.push({ id, pts }), remove: () => {}, redraw: () => {},
  snap: (x, y) => (snapped.push([x, y]), [x + 0.5, y]),
});
ok3(!ed.press(100, 100) && sel === 's', 'a press on the symbol selects it');
const grips = ed.handles();
ok3(grips.length === 3 && grips[2][0] === 113 && grips[2][1] === 100, `a grip on the label, got ${JSON.stringify(grips)}`);
ok3(ed.press(113, 100) && ed.move(200, 160) && snapped.length > 0, 'the label grip is picked up and its drag snaps');
ok3(ed.release() && commits.length === 1 && commits[0].pts.length === 3 && commits[0].pts[2][0] === 200.5 && commits[0].pts[0][0] === 100, 'the drop saves the moved note, symbol unmoved');
// moving the symbol carries its label (and so its leader) along
sym[0].points = commits[0].pts;
ok3(ed.press(100, 100) && ed.move(120, 100) && ed.release(), 'the symbol is dragged');
const moved = commits[commits.length - 1].pts;
ok3(moved[0][0] === 120 && moved[2][0] === 220.5 && moved[2][1] === 160, `the note keeps its place relative to the symbol, got ${JSON.stringify(moved)}`);
const failed = bad + bad2 + bad3;
console.log(failed ? 'FAILED' : 'label leader checks ok');
process.exit(failed ? 1 : 0);
