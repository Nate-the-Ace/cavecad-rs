<script lang="ts">
  // Settings > Sketch (phone): the drawing preferences every drawing tool reads (from the ribbon's own defs in
  // lib/commands.ts, so ids, ranges and defaults stay in step), which symbol standard the picker shows, clearing the
  // favorite symbols (a second tap confirms), and whether the Sketch screen shows the live shot strip.
  import { cave, option, setOption } from '../lib/cave.svelte';
  import { drawingDefaults } from '../lib/commands';
  import { clearFavorites, symbolFavorites } from '../lib/symbol-favorites.svelte';
  import { phonePrefs, setLiveStrip, setSymbolStandard } from '../lib/phone-prefs.svelte';
  import './settings.css';

  const defs = drawingDefaults();
  let confirming = $state(false);
  let confirmTimer: ReturnType<typeof setTimeout> | undefined;
  function clear() {
    clearTimeout(confirmTimer);
    if (!confirming) {
      confirming = true;
      confirmTimer = setTimeout(() => (confirming = false), 4000);
      return;
    }
    confirming = false;
    clearFavorites();
  }
  $effect(() => () => clearTimeout(confirmTimer));
</script>

<div class="set">
  {#each defs as d (d.id)}
    {#if d.kind === 'range'}
      {@const v = option(d.id, d.default)}
      <label class="row range">
        <span class="lab">{d.label}</span>
        <input type="range" min={d.min} max={d.max} step={d.step} value={v} oninput={(e) => setOption(d.id, Number(e.currentTarget.value))} aria-label={d.label} aria-valuetext={d.show(v, cave.units)} />
        <span class="val">{d.show(v, cave.units)}</span>
      </label>
    {:else if d.kind === 'toggle'}
      <label class="chk"><input type="checkbox" checked={option(d.id, d.default)} onchange={(e) => setOption(d.id, e.currentTarget.checked)} /> {d.label}</label>
    {/if}
  {/each}
  <div class="row">
    <span class="lab">Symbols</span>
    <div class="seg" role="group" aria-label="Symbol standard">
      <button class:on={phonePrefs.standard === 'nss'} aria-pressed={phonePrefs.standard === 'nss'} onclick={() => setSymbolStandard('nss')}>NSS</button>
      <button class:on={phonePrefs.standard === 'uis'} aria-pressed={phonePrefs.standard === 'uis'} onclick={() => setSymbolStandard('uis')}>UIS</button>
    </div>
  </div>
  <button class="clear" class:warn={confirming} onclick={clear} disabled={!confirming && symbolFavorites.keys.length === 0}>
    {confirming ? 'Tap again to clear' : `Clear favorites (${symbolFavorites.keys.length})`}
  </button>
  <label class="chk"><input type="checkbox" checked={phonePrefs.liveStrip} onchange={(e) => setLiveStrip(e.currentTarget.checked)} /> Show live shot strip</label>
</div>

<style>
  .range input[type='range'] { flex: 1 1 auto; min-height: var(--p-row); margin: 0; accent-color: var(--accent); }
  .range .val { flex: 0 0 76px; text-align: right; font-size: var(--p-font-small); color: var(--muted); }
  .clear { flex: none; }
  .set .clear.warn { background: #e5484d; border-color: #e5484d; color: #fff; font-weight: 600; }
</style>
