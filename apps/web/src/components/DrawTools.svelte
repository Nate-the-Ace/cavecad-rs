<script lang="ts">
  // The drawing tools. Pick one, then draw in whichever view you like: the line goes to that view's
  // context (the open profile, the cross-section station, the map plan). Pick it again, or press
  // Escape, to put it down.
  import { BrickWall, Minus, CircleDashed, Pencil, Route, Droplets, Waves, TrendingDown, Type, Ellipsis, Equal, MoveHorizontal, ArrowDownToLine, Slash, Zap, AlignJustify, Mountain } from '@lucide/svelte';
  import { cave, setTool } from '../lib/cave.svelte';
  import { engine, loadEngine, type TypeStyle } from '../lib/engine';

  /** Which group of tools this shows (the Draw tab has one ribbon group for each). */
  let { group = 'Linework' }: { group?: string } = $props();

  let tools = $state<TypeStyle[]>([]);
  // Re-read when the group changes: the collapsed menu reuses this one component for every group.
  $effect(() => {
    const wanted = group;
    loadEngine().then(() => (tools = engine.drawToolGroups().find((g) => g.name === wanted)?.tools ?? []));
  });
  const icons: Record<string, typeof Route> = { wall: BrickWall, ledge: Minus, pit: CircleDashed, sketch: Pencil, flowstone: Droplets, rimstone: Waves, slope: TrendingDown, text: Type, glyphs: Mountain, underlying: Ellipsis, presumed: Equal, narrow: MoveHorizontal, dripline: ArrowDownToLine, joint: Slash, fault: Zap, bedding: AlignJustify, contour: Mountain };
</script>

<div class="tools" role="group" aria-label="Drawing tools">
  {#each tools as t}
    {@const Icon = icons[t.key] ?? Route}
    <button class="tool" class:pressed={cave.tool === t.key} aria-pressed={cave.tool === t.key} title={cave.tool === t.key ? `Put ${t.label} down (Esc)` : `Draw ${t.label}: then draw in the Map, Profile or Cross Section`} onclick={() => setTool(t.key)}>
      <Icon size={22} strokeWidth={1.6} />
      <span>{t.label}</span>
    </button>
  {/each}
</div>

<style>
  .tools { display: flex; align-items: center; gap: 4px; height: 100%; }
  .tool { display: flex; flex-direction: column; align-items: center; gap: 4px; min-width: 58px; box-sizing: border-box; height: calc(var(--ribbon-body) - 1px); justify-content: center; overflow: hidden; padding: 4px 8px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; color: var(--text); }
  .tool:hover { background: var(--panel-2); }
  .tool.pressed { background: var(--accent-bg); outline: 1px solid var(--accent); }
</style>
