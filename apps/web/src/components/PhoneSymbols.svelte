<script lang="ts">
  // The symbol picker as a tall bottom sheet (about 70% of the canvas) for the phone Sketch tab: sticky search, a
  // Favorites section on top, collapsible category sections, 4-across tiles with a star each. Picking a tile arms it
  // (like the desktop SymbolsPanel) and closes the sheet. A long press on a tile also toggles its favorite.
  import { onMount } from 'svelte';
  import { cave, setTool } from '../lib/cave.svelte';
  import { engine, loadEngine } from '../lib/engine';
  import type { SymbolDef } from '../lib/symbols';
  import { paletteGroups, readStandard, symbolTitle, thumb, visibleSymbols } from '../lib/symbol-palette';
  import { isFavorite, sectionCollapsed, setAllSections, symbolFavorites, toggleFavorite, toggleSection } from '../lib/symbol-favorites.svelte';
  import { ChevronRight, Star } from '@lucide/svelte';

  let { onClose }: { onClose: () => void } = $props();
  let all = $state<SymbolDef[]>([]);
  let query = $state('');
  onMount(() => { loadEngine().then(() => (all = engine.symbols())); });
  const groups = $derived(paletteGroups(visibleSymbols(all, query, readStandard()), '', symbolFavorites.keys));

  // Long press (500ms, finger still) toggles a tile's favorite. Moving more than 8px, lifting or a cancel (the browser
  // cancels the pointer when a scroll starts) aborts it; the click after a completed long press is swallowed.
  let pressTimer: ReturnType<typeof setTimeout> | undefined;
  let pressFrom = { x: 0, y: 0 };
  let longPressed = false;
  const endPress = () => { clearTimeout(pressTimer); pressTimer = undefined; };
  function pressStart(e: PointerEvent, key: string) {
    if (e.pointerType === 'mouse') return;
    longPressed = false;
    pressFrom = { x: e.clientX, y: e.clientY };
    endPress();
    pressTimer = setTimeout(() => { pressTimer = undefined; longPressed = true; toggleFavorite(key); }, 500);
  }
  function pressMove(e: PointerEvent) {
    if (pressTimer && Math.hypot(e.clientX - pressFrom.x, e.clientY - pressFrom.y) > 8) endPress();
  }

  const searching = $derived(query.trim() !== '');
  const names = $derived(groups.map((g) => g.name));
  const allClosed = $derived(groups.every((g) => sectionCollapsed(g.name, true, false)));

  function pick(key: string) {
    if (longPressed) { longPressed = false; return; }
    cave.tool = key; // set, not toggled: an already armed symbol stays armed
    onClose();
  }
  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Escape') { e.stopImmediatePropagation(); onClose(); }
  };
  $effect(() => {
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  });
</script>

<div class="sheet" role="dialog" aria-label="Symbols">
  <header>
    <input type="search" placeholder="Search symbols" bind:value={query} aria-label="Search symbols" />
    <button class="fold" disabled={searching} onclick={() => setAllSections(names, !allClosed)}>{allClosed ? 'Expand all' : 'Collapse all'}</button>
    <button class="close" onclick={onClose}>Cancel</button>
  </header>
  <div class="scroll">
    {#each groups as g (g.name)}
      {@const closed = sectionCollapsed(g.name, true, searching)}
      <h3><button class="head" aria-expanded={!closed} onclick={() => toggleSection(g.name, names, true)}><span class="chev" class:open={!closed}><ChevronRight size={16} /></span>{g.name} ({g.items.length})</button></h3>
      {#if !closed}<div class="grid">
        {#each g.items as s (s.key)}
          <div class="cell">
            <button class="tile" class:armed={cave.tool === s.key} aria-pressed={cave.tool === s.key} title={symbolTitle(s)} onclick={() => pick(s.key)}
              onpointerdown={(e) => pressStart(e, s.key)} onpointermove={pressMove} onpointerup={endPress} onpointercancel={endPress} oncontextmenu={(e) => e.preventDefault()}>
              <canvas use:thumb={s} aria-hidden="true"></canvas>
              <span>{s.name}{#if s.set !== 'nss'} <small>{s.set.toUpperCase()}</small>{/if}</span>
            </button>
            <button class="star" class:on={isFavorite(s.key)} aria-pressed={isFavorite(s.key)} aria-label={isFavorite(s.key) ? `Unfavorite ${s.name}` : `Favorite ${s.name}`} onclick={(e) => { e.stopPropagation(); toggleFavorite(s.key); }}>
              <Star size={14} fill={isFavorite(s.key) ? 'currentColor' : 'none'} />
            </button>
          </div>
        {/each}
      </div>{/if}
    {:else}
      <p class="none">{all.length ? `No symbol matches “${query}”.` : 'Loading symbols…'}</p>
    {/each}
  </div>
</div>

<style>
  .sheet { position: absolute; left: 0; right: 0; bottom: 0; height: 70%; display: flex; flex-direction: column; background: var(--panel); border-top: 1px solid var(--line); z-index: 30; touch-action: pan-y; }
  header { display: flex; align-items: center; gap: var(--p-gap); padding: var(--p-gap) var(--p-pad); border-bottom: 1px solid var(--line); flex: none; }
  input { flex: 1; min-width: 0; height: var(--p-row); box-sizing: border-box; font: inherit; font-size: var(--p-font-input); padding: 0 8px; border: 1px solid var(--line); border-radius: var(--p-radius); background: var(--panel-2); color: var(--text); }
  .close, .fold { height: var(--p-row); padding: 0 8px; border: 1px solid var(--line); border-radius: var(--p-radius); background: var(--panel-2); color: var(--text); font: inherit; font-size: var(--p-font-small); touch-action: manipulation; }
  .fold:disabled { opacity: 0.5; }
  .scroll { flex: 1; min-height: 0; overflow-y: auto; padding: 0 var(--p-pad) calc(var(--p-pad) + env(safe-area-inset-bottom, 0px)); overscroll-behavior: contain; }
  .head { display: flex; align-items: center; gap: 6px; width: 100%; min-height: var(--p-hit); padding: 0; background: none; border: 0; color: inherit; font: inherit; text-transform: inherit; letter-spacing: inherit; text-align: left; touch-action: manipulation; }
  .chev { display: inline-flex; transition: transform 0.12s; }
  .chev.open { transform: rotate(90deg); }
  h3 { margin: 0; font-size: 12px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); }
  .grid { display: grid; grid-template-columns: repeat(4, minmax(52px, 1fr)); gap: var(--p-gap); }
  .cell { position: relative; display: flex; flex-direction: column; }
  .cell .tile { flex: 1; -webkit-touch-callout: none; user-select: none; -webkit-user-select: none; }
  /* 32pt hit area, drawn small in the tile's top-right corner */
  .star { position: absolute; top: -2px; right: -2px; display: grid; place-items: center; width: 32px; height: 32px; padding: 0; border: 0; background: none; color: var(--muted); }
  .star.on { color: var(--accent); }
  .tile { display: flex; flex-direction: column; align-items: center; justify-content: flex-start; gap: 3px; min-height: 52px; padding: 6px 2px; background: none; border: 1px solid transparent; border-radius: var(--p-radius); color: var(--text); font-size: 11px; text-align: center; line-height: 1.15; overflow-wrap: anywhere; }
  .tile.armed { background: var(--accent-bg); border-color: var(--accent); }
  .tile canvas { width: 44px; height: 44px; color: var(--text); }
  small { display: block; font-size: 9px; color: var(--muted); letter-spacing: 0.05em; }
  .none { color: var(--muted); font-size: var(--p-font); }
</style>
