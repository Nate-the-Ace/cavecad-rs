<script lang="ts">
  // The phone's Settings tab: a segmented row of sections from the registry (lib/settings-sections.ts), one shown below.
  import { registerDefaultSettings } from '../lib/settings-default';
  import { settingsSections } from '../lib/settings-sections';

  registerDefaultSettings();
  const sections = settingsSections();
  let current = $state(sections.find((s) => s.id === 'account')?.id ?? sections[0]?.id ?? '');
  const shown = $derived(sections.find((s) => s.id === current));
</script>

<div class="settings">
  <div class="bar" role="tablist" aria-label="Settings sections">
    {#each sections as s (s.id)}
      <button role="tab" aria-selected={current === s.id} class:on={current === s.id} onclick={() => (current = s.id)}>{s.label}</button>
    {/each}
  </div>
  <div class="body">
    {#if shown}{@const C = shown.component}<C />{/if}
  </div>
</div>

<style>
  .settings { display: flex; flex-direction: column; height: 100%; }
  .bar { display: flex; gap: 2px; padding: var(--p-gap); flex: none; overflow-x: auto; scrollbar-width: none; background: var(--panel); border-bottom: 1px solid var(--line); }
  .bar::-webkit-scrollbar { display: none; }
  .bar button { flex: none; min-height: 36px; padding: 0 10px; border: 0; border-radius: var(--p-radius); background: var(--panel-2); color: var(--text); font: inherit; font-size: var(--p-font-small); cursor: pointer; touch-action: manipulation; }
  .bar button.on { background: var(--accent); color: #fff; font-weight: 600; }
  .body { flex: 1; min-height: 0; overflow: hidden; }
</style>
