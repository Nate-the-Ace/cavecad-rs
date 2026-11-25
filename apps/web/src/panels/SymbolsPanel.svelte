<script lang="ts">
  // The symbol palette: every point symbol as a picture of itself, filed by category. Click one to
  // arm it, then click (or click and drag, to aim and size it) in the Map, Profile or Cross Section.
  import { onMount } from 'svelte';
  import { cave, setTool } from '../lib/cave.svelte';
  import { engine, loadEngine } from '../lib/engine';
  import type { SymbolDef } from '../lib/symbols';
  import { paletteGroups, readStandard, STANDARD_KEY, symbolTitle, thumb, visibleSymbols, type Standard } from '../lib/symbol-palette';
  import { isFavorite, sectionCollapsed, symbolFavorites, toggleFavorite, toggleSection } from '../lib/symbol-favorites.svelte';
  import { ChevronRight, Star } from '@lucide/svelte';

  let all = $state<SymbolDef[]>([]);
  let query = $state('');
  onMount(() => {
    loadEngine().then(() => (all = engine.symbols()));
  });

  let standard = $state<Standard>(readStandard());
  function setStandard(s: Standard) {
    standard = s;
    try {
      localStorage.setItem(STANDARD_KEY, s);
    } catch {
      /* remembered for this visit only */
    }
  }
  const shown = $derived(visibleSymbols(all, query, standard));
  // Favorites first (the user's stars, kept on this device), then the categories; each section folds.
  const groups = $derived(paletteGroups(shown, '', symbolFavorites.keys));
</script>

<div class="panel">
  <div class="bar">
    <input type="search" placeholder="Search symbols" bind:value={query} aria-label="Search symbols" />
    <div class="standard" role="radiogroup" aria-label="Symbol standard">
      <button role="radio" aria-checked={standard === 'nss'} class:on={standard === 'nss'} title="The NSS symbols used on most US cave maps" onclick={() => setStandard('nss')}>US (NSS)</button>
      <button role="radio" aria-checked={standard === 'uis'} class:on={standard === 'uis'} title="The UIS symbols used on cave maps in much of the rest of the world" onclick={() => setStandard('uis')}>International (UIS)</button>
    </div>
    <span class="hint">{cave.tool.startsWith('sym-') ? 'Click to place, drag to aim and size. Esc to stop.' : 'Pick a symbol, then click in a view.'}</span>
  </div>
  <div class="scroll">
    {#each groups as g (g.name)}
      {@const closed = sectionCollapsed(g.name, false, query.trim() !== '')}
      <h3><button class="head" aria-expanded={!closed} onclick={() => toggleSection(g.name, groups.map((x) => x.name), false)}><span class="chev" class:open={!closed}><ChevronRight size={14} /></span>{g.name} ({g.items.length})</button></h3>
      {#if !closed}<div class="grid">
        {#each g.items as s (s.key)}
          <div class="cell">
            <button class="tile" class:armed={cave.tool === s.key} aria-pressed={cave.tool === s.key} title={symbolTitle(s)} onclick={() => setTool(s.key)}>
              <canvas use:thumb={s} aria-hidden="true"></canvas>
              <span>{s.name}{#if s.set !== standard} <small>{s.set.toUpperCase()}</small>{/if}</span>
            </button>
            <button class="star" class:on={isFavorite(s.key)} aria-pressed={isFavorite(s.key)} aria-label={isFavorite(s.key) ? `Unfavorite ${s.name}` : `Favorite ${s.name}`} title={isFavorite(s.key) ? 'Remove from Favorites' : 'Add to Favorites'} onclick={(e) => { e.stopPropagation(); toggleFavorite(s.key); }}>
              <Star size={13} fill={isFavorite(s.key) ? 'currentColor' : 'none'} />
            </button>
          </div>
        {/each}
      </div>{/if}
    {:else}
      <p class="none">No symbol matches “{query}”.</p>
    {/each}
  </div>
</div>

<style>
  .panel { display: flex; flex-direction: column; height: 100%; background: var(--panel); color: var(--text); }
  .bar { padding: 8px 10px; display: flex; flex-direction: column; gap: 4px; border-bottom: 1px solid var(--line); }
  input { font: inherit; padding: 5px 8px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); }
  .hint { font-size: 11px; color: var(--muted); }
  .standard { display: flex; gap: 4px; }
  .standard button { flex: 1; font: inherit; font-size: 11px; padding: 3px 6px; border: 1px solid var(--line); border-radius: var(--radius); background: none; color: var(--text); cursor: pointer; }
  .standard button.on { background: var(--accent-bg); border-color: var(--accent); }
  .scroll { overflow: auto; padding: 4px 10px 12px; flex: 1; }
  .head { display: flex; align-items: center; gap: 4px; width: 100%; padding: 4px 0; background: none; border: 0; color: inherit; font: inherit; text-transform: inherit; letter-spacing: inherit; cursor: pointer; text-align: left; }
  .chev { display: inline-flex; transition: transform 0.12s; }
  .chev.open { transform: rotate(90deg); }
  .cell { position: relative; display: flex; flex-direction: column; }
  .cell .tile { flex: 1; }
  .star { position: absolute; top: 1px; right: 1px; display: grid; place-items: center; width: 20px; height: 20px; padding: 0; border: 0; border-radius: 50%; background: none; color: var(--muted); cursor: pointer; opacity: 0; }
  .cell:hover .star, .star:focus-visible, .star.on { opacity: 1; }
  .star.on { color: var(--accent); }
  h3 { margin: 10px 0 6px; font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(78px, 1fr)); gap: 6px; }
  .tile { display: flex; flex-direction: column; align-items: center; gap: 3px; padding: 6px 4px; background: none; border: 1px solid transparent; border-radius: var(--radius); cursor: pointer; color: var(--text); font-size: 11px; text-align: center; line-height: 1.15; }
  .tile:hover { background: var(--panel-2); }
  .tile.armed { background: var(--accent-bg); border-color: var(--accent); }
  .tile canvas { width: 44px; height: 44px; color: var(--text); }
  small { display: block; font-size: 9px; color: var(--muted); letter-spacing: 0.05em; }
  .none { color: var(--muted); font-size: 13px; }
</style>
