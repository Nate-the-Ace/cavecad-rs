<script lang="ts">
  // A canvas that paints what `draw` says, in the panel's ink, and again when the theme or the width changes.
  import { onTheme } from '../../lib/theme';

  let { draw, height = 90 }: { draw: (g: CanvasRenderingContext2D, w: number, h: number) => void; height?: number } = $props();
  let canvas: HTMLCanvasElement;
  let width = $state(300);
  let theme = $state(0);

  $effect(() => {
    const off = onTheme(() => theme++);
    const watch = new ResizeObserver(() => (width = Math.max(100, Math.floor(canvas.clientWidth))));
    watch.observe(canvas);
    return () => {
      off();
      watch.disconnect();
    };
  });

  $effect(() => {
    void theme;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = width * dpr;
    canvas.height = height * dpr;
    const g = canvas.getContext('2d')!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, width, height);
    const ink = getComputedStyle(canvas).color;
    g.strokeStyle = ink;
    g.fillStyle = ink;
    g.lineWidth = 1;
    g.lineCap = 'butt';
    g.lineJoin = 'round';
    draw(g, width, height);
  });
</script>

<canvas bind:this={canvas} style="height:{height}px" aria-hidden="true"></canvas>

<style>
  canvas { width: 100%; display: block; color: var(--text); background: var(--panel-2); border: 1px solid var(--line); border-radius: var(--radius); }
</style>
