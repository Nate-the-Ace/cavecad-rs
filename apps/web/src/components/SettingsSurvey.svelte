<script lang="ts">
  // Settings > Survey (phone): what a shot is (the desktop ribbon's Shots group: Leg, Splay or LRUD, or LRUD only for
  // an instrument that groups legs itself), what the one Take shot button takes with an X1 (one shot, or station to
  // station), which sweep the Sweep button starts, backsights, write at once or on Accept, the units, and what the
  // X1's marks draw (folded away).
  import X1Marks from './X1Marks.svelte';
  import HardwareKeysSettings from './HardwareKeysSettings.svelte';
  import * as live from '../lib/survey.svelte';
  import { survey } from '../lib/survey.svelte';
  import { cave, setUnits } from '../lib/cave.svelte';
  import { setBacksightsShown } from '../lib/notebook-display.svelte';
  import { surveyBacksightsChange } from '../lib/notebook-display.ts';
  import { sweepName } from '../lib/phone-survey';
  import './settings.css';

  const manual = $derived(survey.standard === 'manual');
  const SHOT_TYPES = [['single', 'Single'], ['leg', 'Station to station']] as const;
  const SWEEPS = ['lrud', 'room'] as const;
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
  {#if live.aimsFirst() || survey.groupsLegs}
    <div class="row">
      <span class="lab">Shot type</span>
      <div class="seg" role="group" aria-label="What Take shot takes">
        {#each SHOT_TYPES as [t, label] (t)}
          <button class:on={survey.shotType === t} aria-pressed={survey.shotType === t} onclick={() => live.setShotType(t)}>{label}</button>
        {/each}
      </div>
    </div>
  {/if}
  <div class="row">
    <span class="lab">Sweep</span>
    <div class="seg" role="group" aria-label="What Sweep starts">
      {#each SWEEPS as k (k)}
        <button class:on={survey.sweepDefault === k} aria-pressed={survey.sweepDefault === k} onclick={() => live.setSweepDefault(k)}>{sweepName(k, manual)}</button>
      {/each}
    </div>
  </div>
  <label class="chk"><input type="checkbox" checked={survey.state.backsights} onchange={(e) => { const c = surveyBacksightsChange(e.currentTarget.checked); live.setBacksights(c.collect); setBacksightsShown(c.show); }} /> Backsights</label>
  <label class="chk"><input type="checkbox" checked={survey.auto} onchange={(e) => live.setAuto(e.currentTarget.checked)} /> Write at once</label>
  <div class="row">
    <span class="lab">Units</span>
    <div class="seg" role="group" aria-label="Units">
      <button class:on={cave.units === 'metric'} aria-pressed={cave.units === 'metric'} onclick={() => setUnits('metric')}>Metric</button>
      <button class:on={cave.units === 'imperial'} aria-pressed={cave.units === 'imperial'} onclick={() => setUnits('imperial')}>Imperial</button>
    </div>
  </div>
  <div class="marks"><X1Marks /></div>
  <HardwareKeysSettings />
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
