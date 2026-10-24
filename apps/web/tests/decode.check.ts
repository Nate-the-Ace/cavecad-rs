import { decodeSurveyText } from '../src/lib/text-decode.ts';
import assert from 'node:assert/strict';

const enc = (s: string) => new TextEncoder().encode(s);
// UTF-8 stays UTF-8
assert.equal(decodeSurveyText(enc('Müller A1 A2')), 'Müller A1 A2');
// a Compass file from Windows: ü is the single byte 0xFC
assert.equal(decodeSurveyText(Uint8Array.from([0x4d, 0xfc, 0x6c, 0x6c, 0x65, 0x72])), 'Müller');
// Therion declaring Latin-2: 0xE8 is č there (è in Windows-1252)
const th = [...enc('encoding iso8859-2\nsurvey '), 0xe8, ...enc('\n')];
assert.equal(decodeSurveyText(Uint8Array.from(th)), 'encoding iso8859-2\nsurvey č\n');
console.log('decode: ok');
