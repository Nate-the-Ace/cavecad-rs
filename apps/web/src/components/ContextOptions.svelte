<script lang="ts">
  // The settings of the running command, as a contextual group in the ribbon.
  import { cave, option, setOption } from '../lib/cave.svelte';
  import { optionsFor } from '../lib/commands';

  const defs = $derived(optionsFor(cave.tool));
</script>

<div class="opts">
  {#each defs as d}
    {#if d.kind === 'range'}
      {@const v = option(d.id, d.default)}
      <label class="opt" title={d.hint}>
        <span class="name">{d.label}</span>
        <input type="range" min={d.min} max={d.max} step={d.step} value={v} oninput={(e) => setOption(d.id, Number(e.currentTarget.value))} aria-label={d.label} aria-valuetext={d.show(v)} />
        <span class="val">{d.show(v)}</span>
      </label>
    {:else}
      <label class="opt toggle" title={d.hint}>
        <input type="checkbox" checked={option(d.id, d.default)} onchange={(e) => setOption(d.id, e.currentTarget.checked)} />
        <span class="name">{d.label}</span>
      </label>
    {/if}
  {/each}
</div>

<style>
  .opts { display: flex; flex-direction: column; gap: 6px; justify-content: center; min-width: 230px; font-size: 12px; }
  .opt { display: flex; align-items: center; gap: 8px; color: var(--text); }
  .name { min-width: 92px; }
  .toggle .name { min-width: 0; }
  input[type='range'] { width: 120px; accent-color: var(--accent); }
  .val { color: var(--muted); min-width: 70px; }
</style>
