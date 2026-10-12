import { angleText, lrudText, parseAngle, parseLrud, suggestNext, withSide } from '../src/lib/notebook.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };

ok(parseLrud('', 'metric')?.value === null && parseLrud('', 'metric')?.open === false, 'blank is not measured');
ok(parseLrud('P', 'metric')?.open === true && parseLrud('p', 'imperial')?.value === null, 'P is open passage, not zero');
ok(parseLrud('0', 'metric')?.value === 0, 'zero is a wall at the station');
ok(Math.abs((parseLrud('10', 'imperial')?.value ?? 0) - 3.048) < 1e-9, 'a bare number is in the display unit');
ok(Math.abs((parseLrud('30 cm', 'imperial')?.value ?? 0) - 0.3) < 1e-9, 'an explicit unit wins');
ok(parseLrud('abc', 'metric') === undefined && parseLrud('-2', 'metric') === undefined, 'junk and negatives are refused');
ok(lrudText(null, true, 'metric') === 'P' && lrudText(null, false, 'metric') === '' && lrudText(3.048, false, 'imperial') === '10', 'cells show what they hold');
ok(parseAngle('', 'compass') === null && parseAngle('--', 'clino') === null, 'blank is not read');
ok(parseAngle('360', 'compass') === 0 && parseAngle('45.5', 'compass') === 45.5 && parseAngle('361', 'compass') === undefined, 'compass is 0 to 360');
ok(parseAngle('-12', 'clino') === -12 && parseAngle('91', 'clino') === undefined && parseAngle('x', 'clino') === undefined, 'clino is -90 to 90');
ok(angleText(45) === '45' && angleText(45.04) === '45' && angleText(null) === '', 'angles show one decimal at most');
const l = withSide(null, 'left', 2, false);
ok(l.left === 2 && l.right === null && !l.left_open, 'a side replaces only itself');
ok(withSide(l, 'left', null, true).left_open === true, 'open replaces a value');

ok(suggestNext('A93', new Set()) === 'A94' && suggestNext('B9', new Set()) === 'B10' && suggestNext('12', new Set()) === '13', 'the next number');
ok(suggestNext('A93', new Set(['A94', 'A95'])) === 'A96', 'a taken name is skipped so a loop is never closed by accident');
ok(suggestNext('D07', new Set()) === 'D08' && suggestNext('C09', new Set()) === 'C10', 'zero padding is kept');
ok(suggestNext('ENT', new Set()) === '' && suggestNext('', new Set()) === '', 'no number, no suggestion');

if (bad) process.exit(1);
console.log('notebook ok');
