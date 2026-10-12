// Where a station's cross-section sketches sit in the Map and Profile, and how they are hit-tested.
// A section is a small drawing in its own plane ([across, up] metres from its station); a view
// draws it at true size beside the station, joined to it by a dashed leader, and the person drags
// it to where they want it. The offset they leave it at is saved with the drawing.

export type Sketches = { points: [number, number][]; widths: number[] | null }[];

/** `[minU, minV, maxU, maxV]` around the strokes (zeros when empty). */
export function sketchBounds(sketches: Sketches): [number, number, number, number] {
  let b: [number, number, number, number] | null = null;
  for (const k of sketches) {
    for (const [u, v] of k.points) {
      b = b ? [Math.min(b[0], u), Math.min(b[1], v), Math.max(b[2], u), Math.max(b[3], v)] : [u, v, u, v];
    }
  }
  return b ?? [0, 0, 0, 0];
}

/** Where an unplaced section sits in the map: just east of its station, clear of the strokes. */
export function defaultMapOffset(sketches: Sketches): [number, number, number] {
  const b = sketchBounds(sketches);
  return [Math.max(0, -b[0]) + 1.5, 0, 0];
}

/** Where an unplaced section sits on the band: just above its station, clear of the strokes. */
export function defaultBandOffset(sketches: Sketches): [number, number] {
  const b = sketchBounds(sketches);
  return [0, Math.max(0, -b[1]) + 2];
}

/** True when the screen point is on a section's strokes' box (grown by `pad` px). `px` is pixels per metre. */
export function hits(sketches: Sketches, anchor: [number, number], px: number, x: number, y: number, pad = 8): boolean {
  const b = sketchBounds(sketches);
  return x >= anchor[0] + b[0] * px - pad && x <= anchor[0] + b[2] * px + pad && y >= anchor[1] - b[3] * px - pad && y <= anchor[1] - b[1] * px + pad;
}

/** Draws strokes (width scaled by pen weight) with `to` mapping section metres to screen pixels. */
export function drawStrokes(g: CanvasRenderingContext2D, sketches: Sketches, to: (u: number, v: number) => [number, number], color: string, base = 2): void {
  g.strokeStyle = color;
  g.lineCap = 'round';
  g.lineJoin = 'round';
  for (const k of sketches) {
    for (let i = 1; i < k.points.length; i++) {
      const a = to(k.points[i - 1][0], k.points[i - 1][1]);
      const b = to(k.points[i][0], k.points[i][1]);
      g.lineWidth = base * (k.widths ? (k.widths[i - 1] + k.widths[i]) / 2 : 1);
      g.beginPath();
      g.moveTo(a[0], a[1]);
      g.lineTo(b[0], b[1]);
      g.stroke();
    }
  }
}
