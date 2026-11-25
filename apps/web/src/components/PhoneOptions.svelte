<script lang="ts">
  // The armed tool's options as a slim strip over the bottom of the Sketch canvas (the canvas is not dimmed or blocked
  // above it): a 40px header (tool name, Done) then ContextOptions (the ribbon's own settings) restyled for the phone.
  // Sizes to its content, capped at 30% of the canvas. Done, or Escape, puts the tool down.
  import ContextOptions from './ContextOptions.svelte';
  import { cave } from '../lib/cave.svelte';
  import { optionsFor } from '../lib/commands';
  import { engine, loadEngine } from '../lib/engine';

  let labels = $state<Record<string, string>>({});
  loadEngine().then(() => {
    const m: Record<string, string> = {};
    for (const g of engine.drawToolGroups()) for (const t of g.tools) m[t.key] = t.label;
    for (const s of engine.symbols()) m[s.key] = s.name;
    labels = m;
  });
  const show = $derived(cave.tool !== '' && optionsFor(cave.tool).length > 0);
  const title = $derived(labels[cave.tool] ?? cave.tool);
  const onKey = (e: KeyboardEvent) => {
    if (e.key === 'Escape' && cave.tool && !(e.target as HTMLElement | null)?.closest?.('[role="dialog"]')) cave.tool = '';
  };
  $effect(() => {
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  });
</script>

{#if show}
  <div class="strip" role="group" aria-label="Tool options">
    <header><strong>{title}</strong><button class="done" onclick={() => (cave.tool = '')}>Done</button></header>
    <div class="body"><ContextOptions /></div>
  </div>
{/if}

<style>
  .strip { position: absolute; left: 0; right: 0; bottom: 0; max-height: 30%; display: flex; flex-direction: column; background: var(--panel); border-top: 1px solid var(--line); z-index: 20; touch-action: pan-y; }
  header { display: flex; align-items: center; justify-content: space-between; min-height: var(--p-row); padding: 0 var(--p-gap) 0 var(--p-pad); flex: none; font-size: var(--p-font); }
  .done { min-height: var(--p-row); min-width: 64px; border: 0; border-radius: var(--p-radius); background: var(--accent); color: #fff; font: inherit; font-weight: 600; touch-action: manipulation; }
  .body { overflow: auto; padding: 0 var(--p-gap) var(--p-gap); }
  /* ContextOptions is sized for the ribbon's fixed-height body: here it flows. Range = label + slider + value on one
     line, toggles = one wrapping row of chips, text fields 40px at 16px. */
  .body :global(.opts) { height: auto; min-width: 0; gap: 2px; font-size: var(--p-font-small); overflow: visible; }
  .body :global(.opt) { height: auto; min-height: var(--p-row); gap: 8px; }
  .body :global(.opt .name) { min-width: 0; width: 90px; flex: none; overflow: hidden; text-overflow: ellipsis; }
  .body :global(.opt.toggle .name) { width: auto; overflow: visible; }
  .body :global(.row) { flex-wrap: wrap; gap: 2px var(--p-gap); }
  .body :global(.opt.toggle) { padding: 0 6px; gap: 3px; border: 1px solid var(--line); border-radius: var(--p-radius); background: var(--panel-2); }
  .body :global(input[type='range']) { flex: 1; width: auto; min-width: 0; min-height: var(--p-row); margin: 0; }
  .body :global(input[type='checkbox']) { width: 18px; height: 18px; margin: 0; }
  .body :global(.words) { height: var(--p-row); font-size: var(--p-font-input); }
  .body :global(.val) { min-width: 0; width: 70px; flex: none; text-align: right; }
</style>
