<script lang="ts">
  // Area fills: one button for each kind of ground (sand, clay, blocks, water, ...). Pick one, then
  // draw all the way around the area in the Map, Profile or Cross Section. The outline is what is
  // stored; the marks are made from it, so reshaping the outline refills it.
  import { onMount } from 'svelte';
  import { Boxes, Shapes, CircleDot, Grip, Layers, Mountain, Waves, Droplet, Droplets, Milk, Feather, Snowflake, Bone, PaintBucket } from '@lucide/svelte';
  import { cave, setTool } from '../lib/cave.svelte';
  import { loadEngine } from '../lib/engine';
  import { fillDefs, type FillDef } from '../lib/fills';

  let defs = $state<FillDef[]>([]);
  onMount(() => {
    loadEngine().then(() => (defs = fillDefs()));
  });
  const icons: Record<string, typeof PaintBucket> = {
    'fill-blocks': Boxes,
    'fill-debris': Shapes,
    'fill-pebbles': CircleDot,
    'fill-sand': Grip,
    'fill-clay': Layers,
    'fill-bedrock': Mountain,
    'fill-water': Waves,
    'fill-sump': Droplet,
    'fill-flowstone': Droplets,
    'fill-moonmilk': Milk,
    'fill-guano': Feather,
    'fill-ice': Snowflake,
    'fill-bones': Bone
  };
</script>

<div class="tools" role="group" aria-label="Area fills">
  {#each defs as d (d.key)}
    {@const Icon = icons[d.key] ?? PaintBucket}
    {@const on = cave.tool === d.key}
    <button class="tool" class:pressed={on} aria-pressed={on} title={on ? `Put ${d.label} down (Esc)` : `${d.label}: ${d.help}. Then draw all the way around the area in the Map, Profile or Cross Section`} onclick={() => setTool(d.key)}>
      <Icon size={22} strokeWidth={1.6} />
      <span>{d.label}</span>
    </button>
  {/each}
</div>

<style>
  .tools { display: flex; align-items: center; gap: 4px; height: 100%; }
  .tool { display: flex; flex-direction: column; align-items: center; gap: 4px; min-width: 58px; box-sizing: border-box; height: calc(var(--ribbon-body) - 1px); justify-content: center; overflow: hidden; padding: 4px 8px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; color: var(--text); }
  .tool:hover { background: var(--panel-2); }
  .tool.pressed { background: var(--accent-bg); outline: 1px solid var(--accent); }
</style>
