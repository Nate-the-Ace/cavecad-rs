<script lang="ts">
  // The grid's settings, in one place for the phone (Settings > Sketch) and the dock (Options > Display): the cell size,
  // how strong the lines are, and a small live preview at the real drawing scale (1 inch of paper to 20 ft), so a change
  // can be judged without going back to the map. The same two stored settings (`sketchGrid`, `gridOpacity`) drive the
  // Map, Profile, Cross Section and sketch pane.
  import { option, setOption } from '../lib/cave.svelte';
  import { DEFAULT_FEET_PER_INCH, GRIDS, GRID_OFF, GRID_OPACITY_DEFAULT, drawGrid, gridChoice, gridCaption, pxPerMetre } from '../lib/sketch-grid';

  const size = $derived(String(option('sketchGrid', '2')));
  const opacity = $derived(Number(option('gridOpacity', GRID_OPACITY_DEFAULT)));
  let canvas: HTMLCanvasElement | undefined = $state();
  const W = 260;
  const H = 120;

  function paint() {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(W * dpr);
    canvas.height = Math.round(H * dpr);
    const g = canvas.getContext('2d')!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const css = getComputedStyle(canvas);
    const text = css.getPropertyValue('--text').trim() || '#000';
    g.fillStyle = css.getPropertyValue('--panel').trim() || '#fff';
    g.fillRect(0, 0, W, H);
    const spec = gridChoice(size);
    const scale = pxPerMetre(DEFAULT_FEET_PER_INCH);
    const t = { scale, tx: 12.5, ty: H - 10.5 };
    if (spec) drawGrid(g, t, W, H, text, spec, opacity);
    // a stand-in passage, so the grid is judged behind a drawing
    g.strokeStyle = text;
    g.lineWidth = 2;
    g.lineJoin = 'round';
    g.beginPath();
    for (const [i, [x, y]] of ([[0, 2], [4, 5], [9, 4], [14, 8], [19, 6], [24, 10]] as [number, number][]).entries()) {
      const px = t.tx + x * 8.4;
      const py = t.ty - y * 8.4;
      if (i) g.lineTo(px, py);
      else g.moveTo(px, py);
    }
    g.stroke();
    g.fillStyle = text;
    for (const [x, y] of [[0, 2], [9, 4], [19, 6]]) {
      g.beginPath();
      g.arc(t.tx + x * 8.4, t.ty - y * 8.4, 3, 0, Math.PI * 2);
      g.fill();
    }
  }
  $effect(() => {
    void size;
    void opacity;
    void canvas;
    paint();
  });
</script>

<div class="grid-set">
  <label class="grow">
    <span class="lab">Grid</span>
    <select value={size} aria-label="Grid cell size" onchange={(e) => setOption('sketchGrid', e.currentTarget.value)}>
      <option value={GRID_OFF}>Off</option>
      {#each GRIDS as g (g.key)}<option value={g.key}>{g.label}</option>{/each}
    </select>
  </label>
  <label class="grow">
    <span class="lab">Strength</span>
    <input type="range" min="0.05" max="1" step="0.05" value={opacity} aria-label="Grid strength" aria-valuetext="{Math.round(opacity * 100)}%" oninput={(e) => setOption('gridOpacity', Number(e.currentTarget.value))} />
    <span class="val">{Math.round(opacity * 100)}%</span>
  </label>
  <canvas bind:this={canvas} style:width="{W}px" style:height="{H}px" aria-label="Grid preview at 1 inch = 20 ft"></canvas>
  <p class="cap">Preview at 1" = 20 ft{#if gridChoice(size)} · {gridCaption(gridChoice(size)!)}{/if}. Applies to the Map, Profile and Cross Section.</p>
</div>

<style>
  .grid-set { display: flex; flex-direction: column; gap: 6px; flex: none; color: var(--text); }
  .grow { display: flex; align-items: center; gap: 8px; min-height: 36px; }
  .lab { flex: 0 0 82px; color: var(--muted); font-size: 13px; }
  .grow select { flex: 1 1 auto; min-width: 0; }
  .grow input[type='range'] { flex: 1 1 auto; min-width: 0; accent-color: var(--accent); }
  .val { flex: 0 0 44px; text-align: right; font-size: 13px; color: var(--muted); }
  canvas { max-width: 100%; border: 1px solid var(--line); border-radius: 8px; align-self: center; touch-action: none; }
  .cap { margin: 0; font-size: 12px; color: var(--muted); }
</style>
