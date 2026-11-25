<script lang="ts">
  // The Sketch tab's drawing controls, one 41px row under the bar: ONE type dropdown (the desktop Draw tab's groups,
  // first option None) and a Symbols button that opens the picker. It is a row, not a floating toolbar, because every
  // view (Map, Profile, Section) already floats its own controls at the top of the canvas. Both set `cave.tool`
  // directly: choosing the current type again does not toggle it off (None does). Favorite chips are PhoneChips.
  import { cave } from '../lib/cave.svelte';
  import type { SymbolDef } from '../lib/symbols';
  import { engine, loadEngine } from '../lib/engine';

  const SYM = '__symbol';
  let { onSymbols }: { onSymbols: () => void } = $props();
  let groups = $state<{ name: string; tools: { key: string; label: string }[] }[]>([]);
  let symbols = $state<SymbolDef[]>([]);
  loadEngine().then(() => {
    groups = engine.drawToolGroups();
    symbols = engine.symbols();
  });
  const isSym = $derived(cave.tool.startsWith('sym-'));
  const armed = $derived(isSym ? symbols.find((s) => s.key === cave.tool) : undefined);
  const value = $derived(cave.tool === '' ? '' : isSym ? SYM : cave.tool);

  function choose(e: Event) {
    const el = e.currentTarget as HTMLSelectElement;
    if (el.value === SYM) return;
    cave.tool = el.value;
  }
</script>

<div class="strip" role="group" aria-label="Drawing tools">
    <select class="type" class:pressed={cave.tool !== ''} aria-label="Drawing type" {value} onchange={choose}>
      <option value="">None</option>
      {#if isSym}<option value={SYM}>Symbol: {armed?.name ?? cave.tool.slice(4)}</option>{/if}
      {#each groups as g (g.name)}
        <optgroup label={g.name}>
          {#each g.tools as t (t.key)}<option value={t.key}>{t.label}</option>{/each}
        </optgroup>
      {/each}
    </select>
    <button class="sym" class:pressed={isSym} aria-haspopup="dialog" onclick={onSymbols}>
      <svg width="18" height="18" viewBox="0 0 18 18" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linejoin="round" aria-hidden="true"><path d="M9 2.5 15.5 14h-13z" /><circle cx="9" cy="10" r="1.2" /></svg>
      <span>Symbols</span>
    </button>
</div>

<style>
  .strip { display: flex; gap: var(--p-gap); padding: 0 var(--p-gap); flex: none; background: var(--panel); border-bottom: 1px solid var(--line); }
  .type, .sym { box-sizing: border-box; height: var(--p-row); border: 1px solid var(--line); background: var(--panel-2); color: var(--text); border-radius: var(--p-radius); touch-action: manipulation; }
  .type { flex: 1; min-width: 0; padding: 0 6px; font: inherit; font-size: var(--p-font-input); }
  .sym { flex: none; display: inline-flex; align-items: center; gap: 5px; padding: 0 12px; font: inherit; font-size: var(--p-font); cursor: pointer; }
  .pressed { background: var(--accent-bg); border-color: var(--accent); color: var(--accent); }
</style>
