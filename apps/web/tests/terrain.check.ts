import { sameTerrain, terrainSampler, thinTerrain, type Terrain } from '../src/lib/terrain.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const near = (a: number | null, b: number) => a !== null && Math.abs(a - b) < 1e-3;

// A 6 x 5 grid, 10 m cells, turned 2 degrees (the convergence), row 0 to the north, on the plane z = 100 + 0.1 x - 0.05 y.
const g = (2 * Math.PI) / 180;
const plane = (x: number, y: number) => 100 + 0.1 * x - 0.05 * y;
function grid(w: number, h: number, hole?: [number, number]): Terrain {
  const xyz = new Float32Array(w * h * 3);
  for (let r = 0; r < h; r++)
    for (let c = 0; c < w; c++) {
      const de = -25 + c * 10;
      const dn = 20 - r * 10;
      const x = de * Math.cos(g) + dn * Math.sin(g);
      const y = -de * Math.sin(g) + dn * Math.cos(g);
      const k = (r * w + c) * 3;
      xyz[k] = x;
      xyz[k + 1] = y;
      xyz[k + 2] = hole && hole[0] === c && hole[1] === r ? NaN : plane(x, y);
    }
  return { w, h, xyz };
}

const t = grid(6, 5);
const at = terrainSampler(t);
ok(near(at(0, 0), plane(0, 0)), 'a point inside the grid lies on the plane');
ok(near(at(7.3, -4.1), plane(7.3, -4.1)), 'between grid points too');
ok(near(at(t.xyz[0], t.xyz[1]), t.xyz[2]), 'the corner point is its own height');
const far = at(1000, 0);
ok(far !== null && Number.isFinite(far), 'outside the grid: clamped to its edge, never null');

const holed = grid(6, 5, [2, 2]);
const hat = terrainSampler(holed);
const k = (2 * 6 + 2) * 3;
const h = hat(holed.xyz[k] + 1, holed.xyz[k + 1] - 1);
ok(h !== null && Number.isFinite(h), 'a hole is left out of the blend, not spread as NaN');

const allHoles: Terrain = { w: 2, h: 2, xyz: new Float32Array([0, 0, NaN, 1, 0, NaN, 0, -1, NaN, 1, -1, NaN]) };
ok(terrainSampler(allHoles)(0.5, -0.5) === null, 'nothing but holes: null');
ok(terrainSampler({ w: 1, h: 1, xyz: new Float32Array([0, 0, 0]) })(0, 0) === null, 'a one-point grid: null');

const thin = thinTerrain(grid(7, 5), 2);
ok(thin.w === 4 && thin.h === 3, 'thinning by 2 keeps every other point and the last row and column');
ok(thin.xyz[thin.xyz.length - 1] === grid(7, 5).xyz[grid(7, 5).xyz.length - 1], 'the far corner survives thinning');

ok(sameTerrain(grid(6, 5, [1, 1]), grid(6, 5, [1, 1])), 'equal grids with equal holes are the same');
ok(!sameTerrain(grid(6, 5), grid(6, 5, [1, 1])), 'a new hole is a change');
ok(sameTerrain(null, null) && !sameTerrain(t, null), 'null compares to null only');

if (bad) {
  console.log(`terrain: ${bad} failed`);
  process.exit(1);
}
console.log('terrain: ok');
