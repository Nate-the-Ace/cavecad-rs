<script lang="ts">
  // The drawing's scale, 1:N. Dashes and hatches are sized on paper at this scale: set it to the scale you will print the map at
  // and a dashed line looks the same on paper at 1:500 or 1:2000 (on screen it grows as you zoom in, like a printed map under a
  // magnifier). Everyone in a shared session sees the same scale, and it is saved with the cave.
  import { cave, changed } from '../lib/cave.svelte';
  import { drawing } from '../lib/line-style.svelte';
  import { engine } from '../lib/engine';
  import { syncDrawing } from '../lib/drawing';

  const PRESETS = [100, 200, 250, 500, 1000, 2000, 2500, 5000];
  const custom = $derived(!PRESETS.includes(drawing.scale));
  let message = $state('');

  function set(n: number) {
    const r = engine.setDrawingScale(n);
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    syncDrawing();
    changed();
  }
</script>

<div class="scale" aria-label="Drawing scale">
  <label title="The scale this drawing is printed at. Dashes and hatches keep their size on paper at this scale.">
    <span>Scale 1:</span>
    <select value={custom ? 'custom' : String(drawing.scale)} aria-label="Drawing scale" disabled={!cave.hasCave} onchange={(e) => e.currentTarget.value !== 'custom' && set(Number(e.currentTarget.value))}>
      {#each PRESETS as n}<option value={String(n)}>{n}</option>{/each}
      <option value="custom">Other…</option>
    </select>
  </label>
  <label title="Any scale from 1:10 to 1:100,000">
    <span>or type</span>
    <input type="number" min="10" max="100000" step="10" value={drawing.scale} aria-label="Drawing scale, any number" disabled={!cave.hasCave} onchange={(e) => set(Number(e.currentTarget.value))} />
  </label>
  {#if message}<span class="msg" role="status">{message}</span>{/if}
</div>

<style>
  .scale { display: flex; flex-direction: column; justify-content: center; gap: 4px; height: 100%; font-size: 12px; }
  label { display: flex; align-items: center; gap: 6px; color: var(--muted); }
  select, input { font: inherit; font-size: 13px; height: 24px; padding: 0 6px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); }
  input { width: 78px; }
  .msg { color: var(--danger); font-size: 11px; }
</style>
