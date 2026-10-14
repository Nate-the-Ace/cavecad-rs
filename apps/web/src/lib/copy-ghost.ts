// What the Copy command draws: the copies faded where the next press would drop them, a line from the base point to the pointer,
// and before the base point is picked a small cross with the prompt. Drawn on a 2D canvas.

import type { ClipLine } from './clip';

const INK = '#e8590c';

/** Fades `draw` (which paints lines onto the canvas it is given) onto `g` at 55% strength. */
function faded(g: CanvasRenderingContext2D, draw: (tg: CanvasRenderingContext2D) => void) {
  const tmp = document.createElement('canvas');
  tmp.width = g.canvas.width;
  tmp.height = g.canvas.height;
  const tg = tmp.getContext('2d');
  if (!tg) return;
  tg.setTransform(g.getTransform());
  draw(tg);
  g.save();
  g.setTransform(1, 0, 0, 1, 0, 0);
  g.globalAlpha = 0.55;
  g.drawImage(tmp, 0, 0);
  g.restore();
}

/** The Copy command's overlay: `base` and `cursor` are screen points. `base` null means it is still to be picked. */
export function paintCopy(g: CanvasRenderingContext2D, base: [number, number] | null, cursor: [number, number] | null, lines: ClipLine[] | null, drawLines: (tg: CanvasRenderingContext2D, lines: ClipLine[]) => void, pasting = false) {
  if (!cursor) return;
  g.save();
  g.lineWidth = 1.5;
  g.strokeStyle = INK;
  g.fillStyle = INK;
  if (pasting) {
    // a paste: the copies faded where a press would put them, and the way to place them
    if (lines) faded(g, (tg) => drawLines(tg, lines));
    g.beginPath();
    g.moveTo(cursor[0] - 7, cursor[1]);
    g.lineTo(cursor[0] + 7, cursor[1]);
    g.moveTo(cursor[0], cursor[1] - 7);
    g.lineTo(cursor[0], cursor[1] + 7);
    g.stroke();
    g.font = '12px system-ui, sans-serif';
    g.fillText('Click to place', cursor[0] + 12, cursor[1] - 10);
    g.restore();
    return;
  }
  if (!base) {
    g.beginPath();
    g.moveTo(cursor[0] - 9, cursor[1]);
    g.lineTo(cursor[0] + 9, cursor[1]);
    g.moveTo(cursor[0], cursor[1] - 9);
    g.lineTo(cursor[0], cursor[1] + 9);
    g.stroke();
    g.font = '12px system-ui, sans-serif';
    g.fillText('Pick a base point', cursor[0] + 12, cursor[1] - 10);
    g.restore();
    return;
  }
  if (lines) faded(g, (tg) => drawLines(tg, lines));
  g.setLineDash([5, 4]);
  g.beginPath();
  g.moveTo(base[0], base[1]);
  g.lineTo(cursor[0], cursor[1]);
  g.stroke();
  g.setLineDash([]);
  g.beginPath();
  g.arc(base[0], base[1], 4, 0, Math.PI * 2);
  g.fill();
  g.beginPath();
  g.arc(cursor[0], cursor[1], 4, 0, Math.PI * 2);
  g.stroke();
  g.restore();
}
