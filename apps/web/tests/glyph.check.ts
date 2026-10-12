import { leaderEnd, glyphCurves } from '../src/lib/section-glyph.ts';
let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
// a square drawing at x 100..120, y 0..20 (screen), as one closed-ish stroke
const sq = [[0, 0], [20, 0], [20, 20], [0, 20], [0, 0]] as [number, number][];
const curves = glyphCurves([{ points: sq, widths: null }], (u, v) => [100 + u, v]);
// the station is to the left: the leader stops on the nearer (left) side of the shell, not inside
const left = leaderEnd([0, 10], curves);
ok(left.end[0] > 95 && left.end[0] < 105, `ends at the left edge, got ${left.end}`);
ok(Math.abs(left.centre[0] - 110) < 1.5 && Math.abs(left.centre[1] - 10) < 1.5, `centre ${left.centre}`);
ok(left.below[1] > 20, 'label sits under the drawing');
// from the right it stops at the right edge
const right = leaderEnd([300, 10], curves);
ok(right.end[0] > 115 && right.end[0] < 125, `ends at the right edge, got ${right.end}`);
// a leader that would miss every stroke (open gap in the drawing) still ends on the drawing
const open = glyphCurves([{ points: [[0, 0], [0, 20]], widths: null }, { points: [[20, 0], [20, 20]], widths: null }], (u, v) => [100 + u, v]);
const miss = leaderEnd([0, 10], open);
ok(Number.isFinite(miss.end[0]) && miss.end[0] > 95 && miss.end[0] < 125, `nearest stroke, got ${miss.end}`);
ok(leaderEnd([5, 5], []).end[0] === 5, 'no strokes: nothing to end on');
console.log(bad ? 'FAILED' : 'glyph checks ok');
process.exit(bad ? 1 : 0);
