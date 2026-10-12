// The rock halo: a soft, light glow that starts at the cave walls and fades into the rock around the
// cave, with small rock marks (angular fragments and dashes) that fade away with it. It is only a
// picture to look at, drawn under everything the survey draws: nothing is stored, and it never
// moves a station or a line. Every view (plan, profile, section) hands in its own wall lines in
// screen pixels, and the passage itself (so the glow stays out of the passage).

/** A wall line (or a closed outline) in screen pixels. */
export interface HaloLine {
  pts: [number, number][];
  closed?: boolean;
}

export interface HaloOptions {
  /** How far the glow reaches from a wall, in pixels. */
  reach: number;
  /** Colour of the glow and of the marks (any CSS colour). */
  tint: string;
  ink: string;
  /** Screen pixels to world units and back (the marks are fixed to the world, so they stay put as you pan). */
  toWorld: (x: number, y: number) => [number, number];
  toScreen: (x: number, y: number) => [number, number];
  /** How quickly the glow dies away: 0 holds strong then drops at the edge, 1 fades fast and trails off (default 0.5, an even fade). */
  fade?: number;
  /** Passage interiors (screen pixels) that the glow must not cover. */
  inside?: [number, number][][];
}

const STEPS = 24;
const PEAK = 0.85; // how strong the glow is right at the wall, 0..1
const SCALE = 0.5; // the glow is soft, so it is made at half size and stretched

let mask: HTMLCanvasElement | null = null;
let tintLayer: HTMLCanvasElement | null = null;
let markLayer: HTMLCanvasElement | null = null;

function layer(c: HTMLCanvasElement | null, w: number, h: number): HTMLCanvasElement {
  const el = c ?? document.createElement('canvas');
  if (el.width !== w || el.height !== h) {
    el.width = w;
    el.height = h;
  }
  return el;
}

/** A repeatable number in [0, 1) for a cell: the same cell always gives the same marks. */
function hash(ix: number, iy: number, k: number, salt: number): number {
  let h = Math.imul(ix, 374761393) ^ Math.imul(iy, 668265263) ^ Math.imul(k + 7, 2147483647) ^ Math.imul(salt + 1, 1274126177);
  h = Math.imul(h ^ (h >>> 13), 1103515245);
  h ^= h >>> 16;
  return (h >>> 0) / 4294967296;
}

function trace(g: CanvasRenderingContext2D, lines: HaloLine[], s: number) {
  g.beginPath();
  for (const l of lines) {
    l.pts.forEach((p, i) => (i === 0 ? g.moveTo(p[0] * s, p[1] * s) : g.lineTo(p[0] * s, p[1] * s)));
    if (l.closed) g.closePath();
  }
}

/** Draws the halo over the context's current picture. `w` and `h` are the view size in CSS pixels. */
export function drawHalo(g: CanvasRenderingContext2D, w: number, h: number, lines: HaloLine[], o: HaloOptions): void {
  if (!lines.length || w < 8 || h < 8 || o.reach < 4) return;
  const mw = Math.max(1, Math.round(w * SCALE));
  const mh = Math.max(1, Math.round(h * SCALE));

  // 1. the glow's strength: the walls drawn over and over, a little wider each time, so every pixel
  // holds more strokes the nearer it is to a wall.
  const m = (mask = layer(mask, mw, mh));
  const mg = m.getContext('2d')!;
  mg.setTransform(1, 0, 0, 1, 0, 0);
  mg.globalCompositeOperation = 'source-over';
  mg.clearRect(0, 0, mw, mh);
  mg.strokeStyle = '#fff';
  mg.lineCap = 'round';
  mg.lineJoin = 'round';
  const fk = Math.pow(2, 1 - 2 * Math.min(1, Math.max(0, o.fade ?? 0.5)));
  const a = 1 - Math.pow(1 - PEAK, 1 / STEPS);
  mg.globalAlpha = a;
  for (let i = 1; i <= STEPS; i++) {
    mg.lineWidth = 2 * o.reach * Math.pow(i / STEPS, 1 / fk) * SCALE;
    trace(mg, lines, SCALE);
    mg.stroke();
  }
  mg.globalAlpha = 1;
  // the passage itself stays clear: the glow belongs to the rock
  const inside = [...(o.inside ?? []), ...lines.filter((l) => l.closed).map((l) => l.pts)];
  if (inside.length) {
    mg.globalCompositeOperation = 'destination-out';
    mg.fillStyle = '#000';
    mg.beginPath();
    for (const poly of inside) {
      poly.forEach((p, i) => (i === 0 ? mg.moveTo(p[0] * SCALE, p[1] * SCALE) : mg.lineTo(p[0] * SCALE, p[1] * SCALE)));
      mg.closePath();
    }
    mg.fill('nonzero');
    mg.globalCompositeOperation = 'source-over';
  }

  // 2. the soft colour
  const t = (tintLayer = layer(tintLayer, mw, mh));
  const tg = t.getContext('2d')!;
  tg.setTransform(1, 0, 0, 1, 0, 0);
  tg.globalCompositeOperation = 'source-over';
  tg.clearRect(0, 0, mw, mh);
  tg.drawImage(m, 0, 0);
  tg.globalCompositeOperation = 'source-in';
  tg.fillStyle = o.tint;
  tg.fillRect(0, 0, mw, mh);
  g.save();
  g.globalAlpha *= 0.34;
  g.drawImage(t, 0, 0, w, h);
  g.restore();

  // 3. the rock marks, then cut to the glow so they fade out with it
  const dpr = 1;
  const k = (markLayer = layer(markLayer, Math.round(w * dpr), Math.round(h * dpr)));
  const kg = k.getContext('2d')!;
  kg.setTransform(1, 0, 0, 1, 0, 0);
  kg.globalCompositeOperation = 'source-over';
  kg.clearRect(0, 0, k.width, k.height);
  drawRock(kg, w, h, o);
  kg.globalCompositeOperation = 'destination-in';
  kg.drawImage(m, 0, 0, k.width, k.height);
  kg.globalCompositeOperation = 'source-over';
  g.drawImage(k, 0, 0, w, h);
}

/** Rock marks over the whole view, fixed to the world; the glow's strength decides which show. */
function drawRock(g: CanvasRenderingContext2D, w: number, h: number, o: HaloOptions) {
  // Cells double in size as you zoom out, so the marks stay about as far apart on screen as the glow needs.
  const o0 = o.toScreen(0, 0);
  const ux = o.toScreen(1, 0);
  const pxPerUnit = Math.hypot(ux[0] - o0[0], ux[1] - o0[1]);
  if (!(pxPerUnit > 0)) return;
  const k = Math.round(Math.log2(26 / pxPerUnit));
  const cell = Math.pow(2, k);
  const corners = [o.toWorld(0, 0), o.toWorld(w, 0), o.toWorld(0, h), o.toWorld(w, h)];
  const x0 = Math.floor(Math.min(...corners.map((c) => c[0])) / cell) - 1;
  const x1 = Math.ceil(Math.max(...corners.map((c) => c[0])) / cell) + 1;
  const y0 = Math.floor(Math.min(...corners.map((c) => c[1])) / cell) - 1;
  const y1 = Math.ceil(Math.max(...corners.map((c) => c[1])) / cell) + 1;
  if ((x1 - x0) * (y1 - y0) > 9000) return; // far too many to see: skip rather than stall
  g.strokeStyle = o.ink;
  g.fillStyle = o.ink;
  g.lineWidth = 1;
  g.lineCap = 'butt';
  g.globalAlpha = 0.7;
  // The usual bedrock symbol: short horizontal bedding lines, each with a vertical joint at one end.
  for (let ix = x0; ix <= x1; ix++) {
    for (let iy = y0; iy <= y1; iy++) {
      if (hash(ix, iy, k, 0) > 0.78) continue; // gaps, so it reads as rock and not as a grid
      const len = cell * (0.55 + 0.9 * hash(ix, iy, k, 1));
      const wx = (ix + 0.6 * hash(ix, iy, k, 2)) * cell;
      const wy = (iy + 0.1 * hash(ix, iy, k, 3)) * cell; // a slight step from row to row
      const a = o.toScreen(wx, wy);
      const b = o.toScreen(wx + len, wy);
      g.beginPath();
      g.moveTo(a[0], a[1]);
      g.lineTo(b[0], b[1]);
      if (hash(ix, iy, k, 4) < 0.7) {
        const end = hash(ix, iy, k, 5) < 0.5 ? 0 : len;
        const drop = cell * (0.3 + 0.4 * hash(ix, iy, k, 6)) * (hash(ix, iy, k, 7) < 0.5 ? 1 : -1);
        const c = o.toScreen(wx + end, wy);
        const d = o.toScreen(wx + end, wy + drop);
        g.moveTo(c[0], c[1]);
        g.lineTo(d[0], d[1]);
      }
      g.stroke();
    }
  }
  g.globalAlpha = 1;
}
