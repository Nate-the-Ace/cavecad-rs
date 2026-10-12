import { tolerancePx, optionsFor, DRAW_OPTIONS } from '../src/lib/commands.ts';
let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
ok(tolerancePx(1) > tolerancePx(5) && tolerancePx(5) > tolerancePx(10), 'more detail means a tighter fit');
ok(Math.abs(tolerancePx(1) - 8) < 1e-9 && tolerancePx(10) < 1, 'ends of the scale');
ok(tolerancePx(-5) === tolerancePx(1) && tolerancePx(99) === tolerancePx(10), 'out of range is clamped');
ok(optionsFor('wall').length === DRAW_OPTIONS.length && optionsFor('').length === 0, 'a running command has options, none running has none');
ok(DRAW_OPTIONS.every((o) => o.id && o.label && o.hint), 'every option is described');
console.log(bad ? 'FAILED' : 'command option checks ok'); process.exit(bad ? 1 : 0);
