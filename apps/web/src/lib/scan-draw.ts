// Drawing a placed scan in a 2D view (Profile, Cross Section): the picture is carried by the fit's
// matrix into the view's metres and from there to the screen, clipped to its outline when it has one.
// Drawn before the lines, so the survey is always on top of it.

import type { ScanPlacement } from './engine';

/** The screen's frame: where the view's origin sits and how many pixels one metre is (y goes up). */
export interface Frame {
  ox: number;
  oy: number;
  scale: number;
}

/** The six numbers taking a scan pixel to the screen, for `ctx.transform`. */
export function screenMatrix(m: ScanPlacement['matrix'], f: Frame): [number, number, number, number, number, number] {
  return [f.scale * m[0], -f.scale * m[3], f.scale * m[1], -f.scale * m[4], f.ox + f.scale * m[2], f.oy - f.scale * m[5]];
}

/** Draws one placed scan at `opacity`, clipped to its trim. */
export function drawScan(g: CanvasRenderingContext2D, img: CanvasImageSource, p: Pick<ScanPlacement, 'matrix' | 'trim'>, f: Frame, opacity: number) {
  g.save();
  g.globalAlpha = opacity;
  g.transform(...screenMatrix(p.matrix, f));
  if (p.trim && p.trim.length >= 3) {
    g.beginPath();
    p.trim.forEach((q, i) => (i ? g.lineTo(q[0], q[1]) : g.moveTo(q[0], q[1])));
    g.closePath();
    g.clip();
  }
  g.drawImage(img, 0, 0);
  g.restore();
}
