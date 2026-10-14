<script lang="ts">
  // The drawing's scale, 1:N. Dashes and hatches are sized on paper at this scale: set it to the scale you will print the map at
  // and a dashed line looks the same on paper at 1:500 or 1:2000 (on screen it grows as you zoom in, like a printed map under a
  // magnifier). Everyone in a shared session sees the same scale, and it is saved with the cave.
  import { cave, changed } from '../lib/cave.svelte';
  import { drawing } from '../lib/line-style.svelte';
  import { engine } from '../lib/engine';
  import { syncDrawing } from '../lib/drawing';

  const PRESETS = [100, 200, 250, 500, 1000, 2000, 2500, 5000];
  const KEY = 'cavecad.customScales';
  function load(): number[] {
    try {
      const v = JSON.parse(localStorage.getItem(KEY) ?? '[]');
      return Array.isArray(v) ? v.filter((n) => Number.isFinite(n) && n >= 10 && n <= 100000) : [];
    } catch {
      return [];
    }
  }
  // scales typed under "Custom" join the list, kept on this machine; the cave's own scale always shows too
  let saved = $state<number[]>(load());
  let typing = $state(false);
  let typed = $state('');
  let message = $state('');
  const options = $derived([...new Set([...PRESETS, ...saved, drawing.scale])].sort((a, b) => a - b));

  function set(n: number) {
    const r = engine.setDrawingScale(n);
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    syncDrawing();
    changed();
  }

  function pick(v: string) {
    if (v === 'custom') {
      typing = true;
      typed = '';
      return;
    }
    set(Number(v));
  }

  function commit() {
    if (!typing) return;
    const n = Math.round(Number(typed));
    typing = false;
    if (!Number.isFinite(n) || n < 10 || n > 100000) {
      message = 'Scale must be between 1:10 and 1:100,000';
      return;
    }
    if (!PRESETS.includes(n) && !saved.includes(n)) {
      saved = [...saved, n];
      try {
        localStorage.setItem(KEY, JSON.stringify(saved));
      } catch {}
    }
    set(n);
  }
</script>

<div class="scale" aria-label="Drawing scale">
  <label title="The scale this drawing is printed at. Dashes and hatches keep their size on paper at this scale.">
    <span>Scale 1:</span>
    {#if typing}
      <!-- svelte-ignore a11y_autofocus -->
      <input type="number" min="10" max="100000" step="1" autofocus bind:value={typed} aria-label="Custom drawing scale" onchange={commit} onblur={commit} onkeydown={(e) => e.key === 'Escape' && (typing = false)} />
    {:else}
      <select value={String(drawing.scale)} aria-label="Drawing scale" disabled={!cave.hasCave} onchange={(e) => pick(e.currentTarget.value)}>
        {#each options as n}<option value={String(n)}>{n}</option>{/each}
        <option value="custom">Custom…</option>
      </select>
    {/if}
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
