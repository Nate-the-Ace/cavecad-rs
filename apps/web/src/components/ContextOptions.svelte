<script lang="ts">
  // The settings of the running command, as a contextual group in the ribbon. Fits the ribbon's
  // fixed body height: a row for each range setting, and the on/off settings side by side on one row.
  import { cave, option, setOption } from '../lib/cave.svelte';
  import { optionsFor } from '../lib/commands';

  const defs = $derived(optionsFor(cave.tool));
  const ranges = $derived(defs.filter((d) => d.kind === 'range'));
  const toggles = $derived(defs.filter((d) => d.kind === 'toggle'));
</script>

<div class="opts">
  {#each ranges as d}
    {#if d.kind === 'range'}
      {@const v = option(d.id, d.default)}
      <label class="opt" title={d.hint}>
        <span class="name">{d.label}</span>
        <input type="range" min={d.min} max={d.max} step={d.step} value={v} oninput={(e) => setOption(d.id, Number(e.currentTarget.value))} aria-label={d.label} aria-valuetext={d.show(v, cave.units)} />
        <span class="val">{d.show(v, cave.units)}</span>
      </label>
    {/if}
  {/each}
  <div class="row">
    {#each toggles as d}
      {#if d.kind === 'toggle'}
        <label class="opt toggle" title={d.hint}>
          <input type="checkbox" checked={option(d.id, d.default)} onchange={(e) => setOption(d.id, e.currentTarget.checked)} />
          <span class="name">{d.label}</span>
        </label>
      {/if}
    {/each}
  </div>
</div>

<style>
  .opts { display: flex; flex-direction: column; gap: 4px; justify-content: center; height: 100%; min-width: 260px; font-size: 12px; overflow: hidden; }
  .row { display: flex; gap: 14px; }
  .opt { display: flex; align-items: center; gap: 8px; height: 22px; color: var(--text); white-space: nowrap; }
  .name { min-width: 92px; }
  .toggle .name { min-width: 0; }
  input[type='range'] { width: 110px; accent-color: var(--accent); }
  .val { color: var(--muted); min-width: 70px; }
</style>
