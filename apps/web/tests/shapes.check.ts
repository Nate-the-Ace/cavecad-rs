import { ARROWHEADS, ENGINE_HEADS, frameOutline, placeHead } from '../src/lib/shapes.ts';
import { ARROW_LABELS, FRAME_LABELS, parseStyle, styleText } from '../src/lib/line-style-text.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };

// every label the ribbon offers is drawn by the library or by the engine
for (const [k] of ARROW_LABELS) ok(k in ARROWHEADS || ENGINE_HEADS.includes(k), 'arrow ' + k + ' is drawable');
for (const [k] of FRAME_LABELS) ok(k === '' || frameOutline(k, [0, 0, 40, 10], 3) !== null, 'frame ' + k + ' is drawable');
ok(frameOutline('', [0, 0, 1, 1], 1) === null, 'no frame, no outline');
ok(frameOutline('underline', [0, 0, 40, 10], 3)!.closed === false, 'an underline is open');
ok(frameOutline('hexagon', [0, 0, 40, 10], 3)!.pts.length === 6, 'a hexagon has six corners');
ok(frameOutline('octagon', [0, 0, 40, 10], 3)!.pts.length === 8, 'an octagon has eight');

// an arrowhead lands on its tip, pointing the way asked
const head = placeHead('stealth', [10, 10], [0, 1], 4)!;
ok(Math.abs(head[0].pts[0][0] - 10) < 1e-9 && Math.abs(head[0].pts[0][1] - 10) < 1e-9, 'the tip is where asked');
ok(head[0].pts.slice(1).every((p) => p[1] < 10), 'the body runs back from the tip');
ok(placeHead('nope', [0, 0], [1, 0], 1) === null, 'unknown head');

// the note switches survive the stored text, in a fixed order
const s = parseStyle(styleText({ ...parseStyle(''), frame: 'hexagon', flags: 'nf' }));
ok(s.frame === 'hexagon' && s.flags === 'fn', 'flags round-trip sorted: ' + s.flags);
ok(styleText({ ...parseStyle(''), flags: 'h' }).endsWith(';h'), 'flags are the last part');
ok(styleText(parseStyle('')) === '', 'plain stays empty');

if (bad) process.exit(1);
console.log('shapes ok');
