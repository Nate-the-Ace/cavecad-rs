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
console.log(bad ? 'FAILED' : 'label leader checks ok');
process.exit(bad ? 1 : 0);
