<script lang="ts">
  // The rock halo's settings: shown when the halo is picked from the View tab.
  import { cave, haloFade, haloReach, option, setOption } from '../lib/cave.svelte';
  import { inUnits, unitName } from '../lib/units';

  let { part }: { part: string; kind?: string } = $props();
  const on = $derived(option('halo', true));
  const reachShown = $derived(+inUnits(haloReach(), cave.units).toFixed(0));
  const per = $derived(haloReach() / (inUnits(haloReach(), cave.units) || 1)); // metres in one display unit
</script>

{#if part === 'main'}
  <div class="halo">
    <label class="check"><input type="checkbox" checked={on} onchange={(e) => setOption('halo', e.currentTarget.checked)} /> Show</label>
    <div class="sliders">
    <label>Distance <input type="range" min="1" max="60" step="1" value={haloReach()} oninput={(e) => setOption('haloReach', +e.currentTarget.value)} disabled={!on} /> <span class="val">{reachShown} {unitName(cave.units)}</span></label>
    <label>Fade <input type="range" min="0" max="1" step="0.05" value={haloFade()} oninput={(e) => setOption('haloFade', +e.currentTarget.value)} disabled={!on} /> <span class="val">{haloFade() < 0.35 ? 'holds, then drops' : haloFade() > 0.65 ? 'quick, long tail' : 'even'}</span></label>
    </div>
  </div>
{/if}

<style>
  .halo { display: flex; align-items: center; gap: 18px; height: 100%; font-size: 12px; }
  .sliders { display: flex; flex-direction: column; gap: 4px; }
  label.check { white-space: nowrap; }
  label { display: flex; align-items: center; gap: 8px; }
  label :global(input[type='range']) { margin: 0; }
  label:not(.check) { min-width: 0; }
  input[type='range'] { width: 150px; }
  .val { color: var(--muted); min-width: 110px; }
</style>
