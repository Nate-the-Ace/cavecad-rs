// What the browser tells the snap engine: the camera of a flat pane, and the things made from a view's lines and stations.
// (What snapping FINDS is Rust, tested in crates/cave-snap.)
import { sceneThings } from '../src/lib/snap-scene.ts';
import { cameraOf, flatCamera, planFrame } from '../src/lib/snap-types.ts';

let bad = 0;
const ok = (c: boolean, m: string) => { if (!c) { bad++; console.log('FAIL', m); } };
const near = (a: number, b: number, e = 1e-9) => Math.abs(a - b) <= e;

/** Applies a column-major view-projection matrix to a world point, the way the engine does. */
function project(cam: { view_proj: number[]; width: number; height: number }, p: [number, number, number]): [number, number] {
  const m = cam.view_proj;
  const c = [0, 1, 2, 3].map((r) => m[r] * p[0] + m[4 + r] * p[1] + m[8 + r] * p[2] + m[12 + r]);
  return [(c[0] / c[3] * 0.5 + 0.5) * cam.width, (0.5 - c[1] / c[3] * 0.5) * cam.height];
}

// A flat pane: px = tx + u * scale, py = ty - v * scale.
{
  const tf = { scale: 12.5, tx: 130, ty: 310 };
  const cam = flatCamera(tf, 640, 480);
  for (const [u, v] of [[0, 0], [3, 4], [-7.5, 2.25], [20, -9]]) {
    const [x, y] = project(cam, [u, v, 0]);
    ok(near(x, tf.tx + u * tf.scale, 1e-6) && near(y, tf.ty - v * tf.scale, 1e-6), `(${u}, ${v}) lands where the pane draws it`);
  }
  ok(cam.width === 640 && cam.height === 480, 'the camera carries the pane size');
}

// A camera from a matrix keeps its numbers as they are.
{
  const m = new Float32Array(16).fill(0.5);
  const cam = cameraOf(m, 800, 600);
  ok(cam.view_proj.length === 16 && cam.view_proj[3] === 0.5 && Array.isArray(cam.view_proj), 'a typed array becomes a plain array of 16');
}

// Things from lines and stations.
{
  const stamp = (k: string) => k.startsWith('sym-') || k === 'text' || k === 'callout';
  const things = sceneThings(
    [
      { id: 'w1', kind: 'wall', points: [[0, 0, 5], [4, 1, 5], [8, 0, 5]], closed: false },
      { id: 'w2', kind: 'wall', points: [[0, 0]], closed: false },
      { id: 'a1', kind: 'fill-sand', points: [[0, 0, 2], [3, 0, 2], [3, 3, 2]], closed: true },
      { id: 's1', kind: 'sym-pit', points: [[2, 3, 1], [2, 4.5, 1]], text: 'P1' },
      { id: 't1', kind: 'text', points: [[9, 9, 0]], text: 'words' },
      { id: 'c1', kind: 'callout', points: [[1, 1, 0], [2, 2, 0], [3, 3, 0]] },
    ],
    [{ id: 'st1', label: 'A4', at: [10, 20, -3] }],
    { stamp },
  );
  const by = (id: string) => things.find((t) => t.id === id);
  ok(by('w2') === undefined, 'a one-point line is not a curve and is left out');
  const w1 = by('w1')!;
  ok(w1.kind === 'line' && w1.geom.type === 'spline' && w1.geom.pts.length === 3 && w1.frame.origin[2] === 5 && w1.label === 'wall', 'a drawn line is a spline in the plan at the height of its first point');
  ok((by('a1')!.geom as { closed?: boolean }).closed === true, 'a closed line says so');
  const s1 = by('s1')!;
  ok(s1.kind === 'symbol' && s1.geom.type === 'point' && s1.geom.at.join() === '2,3,1' && s1.label === 'P1', 'a symbol offers only its insertion point, with its label');
  ok(by('t1')!.kind === 'text' && by('c1')!.kind === 'leader' && (by('c1')!.geom as { at: number[] }).at.join() === '1,1,0', 'text and leaders offer their first point');
  const st = by('st1')!;
  ok(st.kind === 'station' && st.label === 'A4' && st.geom.type === 'point' && st.geom.at.join() === '10,20,-3', 'a station is a point at its place, named');
  ok(things.length === 6, 'one thing per line that is a place or a curve, and one per station');
  const flat = sceneThings([{ id: 'p', kind: 'wall', points: [[0, 0], [1, 1]] }], [], { stamp, frameOf: () => planFrame(0) });
  ok(flat[0].frame.origin[2] === 0, 'a pane gives its own frame');
}

if (bad) process.exit(1);
console.log('snap ok');
