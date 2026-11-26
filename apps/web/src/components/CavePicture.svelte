<script lang="ts">
  // A cave version drawn in plan, small: the survey's legs and walls, the lines drawn on the map and the stations, fitted
  // to the box with north up. Used by the history list to show the cave as it was at an edit. It draws a picture it is
  // handed and nothing else.
  import type { CavePicture } from '../lib/engine';

  let { picture, height = 220 }: { picture: CavePicture | null; height?: number } = $props();
  let canvas: HTMLCanvasElement | undefined = $state();
  let width = $state(320);

  $effect(() => {
    const p = picture;
    const c = canvas;
    const w = width;
    if (!c) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = Math.round(w * dpr);
    c.height = Math.round(height * dpr);
    const g = c.getContext('2d')!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const css = getComputedStyle(c);
    const ink = css.getPropertyValue('--text').trim() || '#ddd';
    const muted = css.getPropertyValue('--muted').trim() || '#999';
    const accent = css.getPropertyValue('--accent').trim() || '#6af';
    g.clearRect(0, 0, w, height);
    if (!p || !p.bounds) {
      g.fillStyle = muted;
      g.font = '12px system-ui, sans-serif';
      g.textAlign = 'center';
      g.fillText('Nothing drawn yet', w / 2, height / 2);
      return;
    }
    const [x0, y0, x1, y1] = p.bounds;
    const pad = 14;
    const scale = Math.min((w - 2 * pad) / Math.max(1e-6, x1 - x0), (height - 2 * pad) / Math.max(1e-6, y1 - y0));
    const ox = (w - (x1 - x0) * scale) / 2 - x0 * scale;
    const oy = (height + (y1 - y0) * scale) / 2 + y0 * scale;
    const X = (x: number) => ox + x * scale;
    const Y = (y: number) => oy - y * scale;
    g.lineJoin = 'round';
    g.lineCap = 'round';
    const run = (pts: number[], closed = false) => {
      if (pts.length < 4) return;
      g.beginPath();
      g.moveTo(X(pts[0]), Y(pts[1]));
      for (let i = 2; i < pts.length; i += 2) g.lineTo(X(pts[i]), Y(pts[i + 1]));
      if (closed) g.closePath();
      g.stroke();
    };
    g.strokeStyle = muted;
    g.globalAlpha = 0.6;
    g.lineWidth = 1;
    for (const w2 of p.walls) run(w2);
    g.globalAlpha = 0.9;
    g.strokeStyle = ink;
    g.lineWidth = 1.2;
    for (const l of p.lines) run(l.pts, l.closed);
    g.globalAlpha = 1;
    g.strokeStyle = accent;
    g.lineWidth = 1.6;
    g.beginPath();
    for (const [ax, ay, bx, by] of p.legs) {
      g.moveTo(X(ax), Y(ay));
      g.lineTo(X(bx), Y(by));
    }
    g.stroke();
    g.fillStyle = ink;
    for (const [sx, sy] of p.stations) {
      g.beginPath();
      g.arc(X(sx), Y(sy), 2, 0, Math.PI * 2);
      g.fill();
    }
  });
</script>

<div class="pic" bind:clientWidth={width} style:height="{height}px">
  <canvas bind:this={canvas} style:width="{width}px" style:height="{height}px" aria-label="The cave at this edit, in plan, north up"></canvas>
</div>

<style>
  .pic { width: 100%; border: 1px solid var(--line); border-radius: 8px; background: var(--panel); overflow: hidden; }
  canvas { display: block; }
</style>
