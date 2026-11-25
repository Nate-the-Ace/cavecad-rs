<script lang="ts">
  // Settings > Survey (phone): what a shot is (the desktop ribbon's Shots group: Leg, Splay or LRUD, or LRUD only for
  // an instrument that groups legs itself), backsights, write at once or on Accept, the units, and what the X1's
  // marks draw (folded away).
  import X1Marks from './X1Marks.svelte';
  import * as live from '../lib/survey.svelte';
  import { survey } from '../lib/survey.svelte';
  import { cave, setUnits } from '../lib/cave.svelte';
  import './settings.css';

  const MODES = [['leg', 'Leg'], ['splay', 'Splay'], ['lrud', 'LRUD']] as const;
</script>

<div class="set">
  <div class="row">
    <span class="lab">Shot</span>
    {#if survey.groupsLegs}
      <label class="chk"><input type="checkbox" checked={survey.state.mode === 'lrud'} onchange={(e) => live.setMode(e.currentTarget.checked ? 'lrud' : 'auto')} /> LRUD shots</label>
    {:else}
      <div class="seg" role="group" aria-label="What the next shot is">
        {#each MODES as [m, label] (m)}
          <button class:on={survey.state.mode === m} aria-pressed={survey.state.mode === m} onclick={() => live.setMode(m)}>{label}</button>
        {/each}
      </div>
    {/if}
  </div>
  <label class="chk"><input type="checkbox" checked={survey.state.backsights} onchange={(e) => live.setBacksights(e.currentTarget.checked)} /> Backsights</label>
  <label class="chk"><input type="checkbox" checked={survey.auto} onchange={(e) => live.setAuto(e.currentTarget.checked)} /> Write at once</label>
  <div class="row">
    <span class="lab">Units</span>
    <div class="seg" role="group" aria-label="Units">
      <button class:on={cave.units === 'metric'} aria-pressed={cave.units === 'metric'} onclick={() => setUnits('metric')}>Metric</button>
      <button class:on={cave.units === 'imperial'} aria-pressed={cave.units === 'imperial'} onclick={() => setUnits('imperial')}>Imperial</button>
    </div>
  </div>
  <div class="marks"><X1Marks /></div>
</div>

<style>
  .marks { flex: none; }
  .marks :global(.marks) { border-radius: var(--p-radius); padding: 0 var(--p-pad); }
  .marks :global(summary) { min-height: var(--p-row); font-size: var(--p-font); }
  .marks :global(.hint) { display: none; }
  .marks :global(.row) { margin: 0 0 var(--p-gap); gap: var(--p-gap); }
  .marks :global(.name) { flex: 0 0 64px; }
  .marks :global(select) { min-height: var(--p-row); font-size: var(--p-font-input); }
</style>
