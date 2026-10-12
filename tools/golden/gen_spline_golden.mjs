// Writes what the web app's own spline code gives for a few curves, for the Rust copy to match:
//   node --experimental-strip-types tools/golden/gen_spline_golden.mjs <out.json>
import { writeFileSync } from 'node:fs';
import { smooth } from '../../apps/web/src/lib/spline.ts';

const curves = {
  open4: { pts: [[0, 0], [3, 2], [5, -1], [9, 4]], closed: false },
  uneven: { pts: [[0, 0], [0.1, 0.1], [8, 1], [8.2, 5], [20, 4]], closed: false },
  loop: { pts: [[0, 0], [4, 0], [4, 4], [0, 4]], closed: true },
  blob: { pts: [[1, 1], [6, 0.5], [9, 5], [5, 9], [-1, 6], [-2, 2]], closed: true },
  tiny: { pts: [[0, 0], [1e-5, 1e-5], [2, 0]], closed: false },
};
const cases = Object.entries(curves).map(([name, c]) => ({ name, ...c, per: 10, out: smooth(c.pts, null, 10, c.closed).pts }));
writeFileSync(process.argv[2], JSON.stringify(cases));
console.log(cases.length, 'curves');
