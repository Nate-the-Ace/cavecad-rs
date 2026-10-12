<script lang="ts">
  // Area fills: pick the kind of ground (sand, clay, blocks, water, ...), press Fill, then draw all
  // the way around the area in the Map, Profile or Cross Section. The outline is what is stored; the
  // marks are made from it, so reshaping the outline refills it.
  import { onMount } from 'svelte';
  import { PaintBucket } from '@lucide/svelte';
  import { cave, setTool } from '../lib/cave.svelte';
  import { loadEngine } from '../lib/engine';
  import { fillDefs, isFillKind, type FillDef } from '../lib/fills';

  let defs = $state<FillDef[]>([]);
  let choice = $state('fill-sand');
  onMount(() => {
    loadEngine().then(() => (defs = fillDefs()));
  });
  // Picking up a fill tool by any other route (a selected fill) keeps the box in step.
  $effect(() => {
    if (isFillKind(cave.tool)) choice = cave.tool;
  });
  const current = $derived(defs.find((d) => d.key === choice));
  const armed = $derived(cave.tool === choice);
  function pick(key: string) {
    choice = key;
    if (isFillKind(cave.tool)) cave.tool = key; // switching kinds while a fill is running carries on with the new one
  }
</script>

<div class="fills">
  <button class="tool" class:pressed={armed} aria-pressed={armed} title={armed ? 'Put the fill down (Esc)' : 'Fill: then draw all the way around an area in the Map, Profile or Cross Section'} onclick={() => setTool(choice)}>
    <PaintBucket size={22} strokeWidth={1.6} />
    <span>Fill</span>
  </button>
  <div class="pick">
    <select value={choice} aria-label="What the area is" onchange={(e) => pick(e.currentTarget.value)}>
      {#each defs as d}<option value={d.key}>{d.label}</option>{/each}
    </select>
    <span class="help" title={current?.help}>{current?.help ?? ''}</span>
  </div>
</div>

<style>
  .fills { display: flex; align-items: center; gap: 8px; height: 100%; }
  .tool { display: flex; flex-direction: column; align-items: center; gap: 4px; min-width: 58px; box-sizing: border-box; height: calc(var(--ribbon-body) - 1px); justify-content: center; overflow: hidden; padding: 4px 8px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; color: var(--text); }
  .tool:hover { background: var(--panel-2); }
  .tool.pressed { background: var(--accent-bg); outline: 1px solid var(--accent); }
  .pick { display: flex; flex-direction: column; gap: 4px; width: 190px; }
  select { font: inherit; font-size: 13px; height: 26px; padding: 0 6px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); }
  .help { font-size: 11px; color: var(--muted); height: 26px; line-height: 13px; overflow: hidden; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; }
</style>
