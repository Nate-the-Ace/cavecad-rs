import { centreOf, clipFrom, decodeClip, encodeClip, moved } from '../src/lib/clip.ts';
let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const clip = clipFrom('map', [
  { kind: 'wall', points: [[0, 0, 5], [10, 4, 5]], widths: [1, 2], closed: false },
  { kind: 'sym-pit', points: [[2, 2, 5], [2, 4, 5], [5, 2, 5]], text: '30' },
]);
ok(clip.lines[0].points[0].length === 2, 'height is left off');
ok(clip.lines[1].text === '30' && clip.lines[1].points.length === 3, 'a symbol keeps its words and its label note');
const c = centreOf(clip);
ok(c[0] === 5 && c[1] === 2, `middle of the box, got ${c}`);
const m = moved(clip.lines, 10, -1);
ok(m[0].points[1][0] === 20 && m[0].points[1][1] === 3 && clip.lines[0].points[1][0] === 10, 'moved copies, original untouched');
const back = decodeClip(encodeClip(clip));
ok(!!back && back.lines.length === 2 && back.view === 'map', 'survives the clipboard');
ok(decodeClip('hello') === null, 'other text is not a copy');
ok(decodeClip('cavecad-clip:{"v":1,"lines":[{"kind":"wall","points":[[0,0],[NaN,1]],"widths":null,"closed":false,"text":"","style":""}]}') === null, 'damaged text is refused');
ok(decodeClip('cavecad-clip:{"v":1,"view":"map","lines":[{"kind":"wall","points":[[0,0],[1,1]],"widths":null,"closed":false,"text":"","style":""}]}')?.lines.length === 1, 'a good one is read');
const lead = clipFrom('map', [{ kind: 'callout', points: [[0, 0], [5, 5]], style: ';1.00;1.00;arrow;;;1.00;0.0;;0=ABC#1', text: 'x' }]);
ok(!lead.lines[0].style.includes('ABC'), 'a leader is not fastened to the original it was copied from');
console.log(bad ? 'FAILED' : 'clip checks ok');
process.exit(bad ? 1 : 0);
