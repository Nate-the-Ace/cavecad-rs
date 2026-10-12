import { parseLength, lengthText } from '../src/lib/units.ts';
const near = (a: number | null, b: number) => a !== null && Math.abs(a - b) < 1e-9;
const cases: [string, 'metric' | 'imperial', number | null][] = [
  ['12.5', 'metric', 12.5], ['12.5', 'imperial', 12.5 * 0.3048], ['30 cm', 'imperial', 0.3], ['250mm', 'metric', 0.25],
  ["5'6\"", 'metric', 5 * 0.3048 + 6 * 0.0254], ['5 ft 6', 'imperial', 5 * 0.3048 + 6 * 0.0254], ['12 ft', 'metric', 12 * 0.3048],
  ['-2', 'metric', -2], ['6 in', 'imperial', 0.1524], ['', 'metric', null], ['abc', 'metric', null], ['1,5', 'metric', null], ['.5', 'metric', 0.5],
];
let bad = 0;
for (const [t, u, want] of cases) { const got = parseLength(t, u); const ok = want === null ? got === null : near(got, want); if (!ok) { bad++; console.log('FAIL', t, u, got, want); } }
console.log(lengthText(10, 'imperial'), lengthText(10, 'metric'), bad ? 'FAILED' : 'all ok');
process.exit(bad ? 1 : 0);
