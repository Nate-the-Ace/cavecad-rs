import { copyName, newElement, newLayer, plain, simplify } from '../src/lib/patterns.ts';
import { PATTERN_PREFIX, patternName, parseStyle, styleText } from '../src/lib/line-style-text.ts';
let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };

// a copy's name never clashes and stays within the engine's 40 characters
ok(copyName('Ticks', ['Ticks']) === 'Ticks copy', 'the first copy is "copy"');
ok(copyName('Ticks', ['Ticks', 'Ticks copy']) === 'Ticks copy 2', 'the next is "copy 2"');
ok(copyName('Ticks copy 2', ['Ticks', 'Ticks copy', 'Ticks copy 2']) === 'Ticks copy 3', 'copying a copy does not stack the word');
ok(copyName('x'.repeat(40), []).length <= 40, 'a long name is cut to fit');

// plain() drops the own flag and detaches from the original
const def = { name: 'A', elements: [{ t: 'dash' as const, mm: 3 }], own: true };
const p = plain(def);
ok(!('own' in p), 'own is not saved');
p.elements[0] = { t: 'gap', mm: 1 };
ok(def.elements[0].t === 'dash', 'a copy is separate');

// new parts start as something the engine accepts
ok(newElement('motif', 'Tick').t === 'motif' && (newElement('dash', '') as { mm: number }).mm > 0.05, 'new elements are usable');
ok(newLayer('tile', 'Tick').t === 'tile' && (newLayer('hatch', '') as { spacing_mm: number }).spacing_mm >= 0.2, 'new layers are usable');

// freehand strokes are thinned to the corners that matter
const wiggle: [number, number][] = [];
for (let i = 0; i <= 50; i++) wiggle.push([i, i < 25 ? i * 0.02 : 0.5 + (i - 25) * 0.9]);
const thin = simplify(wiggle, 1.5);
ok(thin.length >= 3 && thin.length <= 5, `a bent stroke keeps its bend (${thin.length} points)`);
ok(thin[0][0] === 0 && thin[thin.length - 1][0] === 50, 'and both ends');
ok(simplify([[0, 0], [1, 1]], 1).length === 2, 'two points stay two');

// a pattern is named in the linetype part of a line's style, and survives the style round trip
ok(patternName('pat:Cave ticks') === 'Cave ticks', 'the pattern name is read');
ok(patternName('dashed') === '' && patternName('') === '' && patternName(undefined) === '', 'other linetypes name no pattern');
const s = parseStyle('pat:Cave ticks;1.50;2.00');
ok(s.lt === PATTERN_PREFIX + 'Cave ticks' && s.scale === 1.5 && s.density === 2, 'a pattern style parses');
ok(styleText(s) === 'pat:Cave ticks;1.50;2.00', 'and writes back the same');

// an area's pattern angle rides as the eighth part of its style and is dropped when zero
const turned = parseStyle('pat:Hatchy;1.00;1.00;;;;1.00;30.0');
ok(turned.angle === 30 && turned.lt === 'pat:Hatchy', 'the pattern angle parses');
ok(styleText({ ...turned, angle: -45 }) === 'pat:Hatchy;1.00;1.00;;;;1.00;-45.0', 'and writes back');
ok(styleText({ ...turned, angle: 0 }) === 'pat:Hatchy;1.00;1.00', 'zero angle is not stored');
ok(parseStyle('dashed;1.00;1.00').angle === 0 && parseStyle('').angle === 0, 'no angle means none');

if (bad) process.exit(1);
console.log('pattern checks ok');
