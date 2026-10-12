// How another person's pointer looks on a canvas: a small arrow in their colour with their name.

/** Draws a remote pointer with its tip at (x, y). */
export function drawCursor(g: CanvasRenderingContext2D, x: number, y: number, name: string, color: string): void {
  g.save();
  g.lineJoin = 'round';
  g.fillStyle = color;
  g.strokeStyle = '#ffffff';
  g.lineWidth = 1.5;
  g.beginPath();
  g.moveTo(x, y);
  g.lineTo(x + 5, y + 16);
  g.lineTo(x + 8, y + 10);
  g.lineTo(x + 15, y + 9);
  g.closePath();
  g.fill();
  g.stroke();
  if (name) {
    g.font = '600 11px system-ui, sans-serif';
    const w = g.measureText(name).width + 10;
    g.fillStyle = color;
    g.beginPath();
    g.roundRect(x + 14, y + 14, w, 18, 5);
    g.fill();
    g.fillStyle = '#ffffff';
    g.textBaseline = 'middle';
    g.fillText(name, x + 19, y + 23.5);
  }
  g.restore();
}
