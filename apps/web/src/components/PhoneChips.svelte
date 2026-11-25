<script lang="ts">
  // Up to six favorite symbols as one-tap round chips (40px) in a column along the left edge of the Sketch canvas,
  // shown only once any symbol is starred. Below the views' own top controls (Map's is the tallest, 117px). Tapping
  // one arms that symbol (set, not toggled).
  import { cave } from '../lib/cave.svelte';
  import type { SymbolDef } from '../lib/symbols';
  import { engine, loadEngine } from '../lib/engine';
  import { thumb } from '../lib/symbol-palette';
  import { favoriteItems } from '../lib/symbol-favorites';
  import { symbolFavorites } from '../lib/symbol-favorites.svelte';

  let symbols = $state<SymbolDef[]>([]);
  loadEngine().then(() => (symbols = engine.symbols()));
  const chips = $derived(favoriteItems(symbols, symbolFavorites.keys).slice(0, 6));
</script>

{#if chips.length}
  <div class="chips" role="group" aria-label="Favorite symbols">
    {#each chips as s (s.key)}
      <button class="chip" class:pressed={cave.tool === s.key} aria-pressed={cave.tool === s.key} aria-label={s.name} title={s.name} onclick={() => (cave.tool = s.key)}>
        <canvas use:thumb={s} aria-hidden="true"></canvas>
      </button>
    {/each}
  </div>
{/if}

<style>
  .chips { position: absolute; left: var(--p-gap); top: 124px; z-index: 10; display: flex; flex-direction: column; gap: var(--p-gap); }
  .chip { box-sizing: border-box; width: var(--p-row); height: var(--p-row); display: grid; place-items: center; padding: 0; border: 1px solid var(--line); border-radius: 50%; background: color-mix(in srgb, var(--panel) 88%, transparent); color: var(--text); touch-action: manipulation; cursor: pointer; }
  .chip canvas { width: 30px; height: 30px; }
  .pressed { background: var(--accent-bg); border-color: var(--accent); color: var(--accent); }
</style>
