// The number-keyboard rule (src/lib/numpad.ts): which keyboard a field asks for, and what its touch keys do.
import { isTap, flipSign, keyText, numpadAttrs, touchKeys } from '../src/lib/numpad.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };

ok(flipSign('') === '-', 'blank becomes a minus, ready for the number');
ok(flipSign('-') === '', 'a lone minus goes back to blank');
ok(flipSign('12.5') === '-12.5', 'a positive number turns negative');
ok(flipSign('-0.4') === '0.4', 'a negative number turns positive');
ok(flipSign('+3') === '-3', 'a written plus turns to minus');
ok(flipSign('  7') === '  -7', 'leading spaces stay where they are');
ok(flipSign(flipSign('-12')) === '-12', 'two presses put it back');

ok(JSON.stringify(numpadAttrs()) === '{"inputmode":"decimal"}', 'a plain number field opens the decimal pad');
ok(numpadAttrs({ integer: true }).inputmode === 'numeric', 'a whole number opens the number pad');
ok(numpadAttrs({ signed: true }).inputmode === 'decimal', 'a signed decimal still opens the decimal pad (the ± key gives the minus)');
ok(numpadAttrs({ signed: true, integer: true }).inputmode === 'numeric', 'a signed whole number opens the number pad');
ok(numpadAttrs({ enter: 'done' }).enterkeyhint === 'done' && numpadAttrs({ enter: 'next' }).enterkeyhint === 'next', 'Enter says what it does');
ok(!('enterkeyhint' in numpadAttrs({})), 'no Enter label unless asked');

ok(touchKeys({ signed: true }, false).length === 0 && touchKeys({ keys: ['P'] }, false).length === 0, 'a mouse shows no keys');
ok(touchKeys({ signed: true }, true).join() === '±', 'a signed field on a touch screen gets ±');
ok(touchKeys({}, true).length === 0, 'an unsigned field gets no ±');
ok(touchKeys({ signed: true, keys: ['P'] }, true).join() === '±,P', 'extra keys follow ±');

ok(keyText('', 'P') === 'P' && keyText('3.2', 'P') === 'P', 'an extra key puts its text in');
ok(keyText('P', 'P') === '' && keyText(' p ', 'P') === '', 'pressing it again clears it');

if (bad) { console.log(`${bad} failed`); process.exit(1); }
console.log('numpad: all good');

// A swipe that began on the ± or P key is not a press (review 2026-11-21: it flipped clino signs while scrolling).
if (!isTap({ x: 10, y: 10 }, { x: 14, y: 16 })) throw new Error('a small wobble is still a tap');
if (isTap({ x: 10, y: 10 }, { x: 10, y: 40 })) throw new Error('a 30 px swipe is not a tap');
console.log('numpad tap rule: ok');
