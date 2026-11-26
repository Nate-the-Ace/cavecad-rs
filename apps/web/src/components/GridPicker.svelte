<script lang="ts">
  // The grid choice for the drawing views (Map, Profile, Cross Section): off, or square sizes of 1, 2, 5 or 10 ft, with a
  // heavy line every 10 ft. One stored setting (`sketchGrid`), so every view shows the same grid.
  import { option, setOption } from '../lib/cave.svelte';
  import { GRIDS, GRID_OFF } from '../lib/sketch-grid';
  let { label = 'Grid' }: { label?: string } = $props();
</script>

<select value={String(option('sketchGrid', '2'))} aria-label={label} title="Grid: the square size, with a heavy line every 10 ft" onchange={(e) => setOption('sketchGrid', e.currentTarget.value)}>
  <option value={GRID_OFF}>Grid off</option>
  {#each GRIDS as g (g.key)}<option value={g.key}>{g.label}</option>{/each}
</select>
