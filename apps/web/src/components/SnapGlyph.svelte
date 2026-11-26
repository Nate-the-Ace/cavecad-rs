<script lang="ts">
  // One snap marker, drawn small: the same shape the map shows when that snap catches, so the settings can show what each
  // mode looks like. Redraws when the colour changes (on or off, light or dark).
  import { glyph } from '../lib/snap-draw';
  import type { SnapMode } from '../lib/snap-types';

  let { mode, color = 'currentColor', size = 30 }: { mode: SnapMode; color?: string; size?: number } = $props();
  let canvas: HTMLCanvasElement | undefined = $state();

  $effect(() => {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(size * dpr);
    canvas.height = Math.round(size * dpr);
    const g = canvas.getContext('2d')!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, size, size);
    const style = getComputedStyle(canvas);
    const name = color.startsWith('var(') ? color.slice(4, -1) : '';
    const ink = name ? style.getPropertyValue(name).trim() || style.color : color === 'currentColor' ? style.color : color;
    glyph(g, mode, size / 2, size / 2, size * 0.3, ink);
  });
</script>

<canvas bind:this={canvas} style:width="{size}px" style:height="{size}px" aria-hidden="true"></canvas>
