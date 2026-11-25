<script lang="ts">
  // The phone's Settings tab: a segmented row of sections from the registry (lib/settings-sections.ts), one shown below.
  // Each section fits one screen; only a data list (the instrument log) scrolls, inside its own box. More than five
  // sections do not fit one row at 375pt with readable labels, so the switcher becomes two rows of 40px buttons
  // (every section stays in sight; nothing hides off the edge as it would in a scrolling row).
  import { registerDefaultSettings } from '../lib/settings-default';
  import { settingsSectionsFor } from '../lib/settings-sections';
  import { phone } from '../lib/phone.svelte';

  registerDefaultSettings();
  const sections = settingsSectionsFor('phone');
  // which one shows is kept in `phone.settings`, so other screens can open a section (the Survey pill opens Connection)
  const current = $derived(sections.some((s) => s.id === phone.settings) ? phone.settings : (sections[0]?.id ?? ''));
  const shown = $derived(sections.find((s) => s.id === current));
  const cols = sections.length > 5 ? Math.ceil(sections.length / 2) : sections.length;
</script>

<div class="settings">
  <div class="bar" role="tablist" aria-label="Settings sections" style:--cols={cols}>
    {#each sections as s (s.id)}
      <button role="tab" aria-selected={current === s.id} class:on={current === s.id} onclick={() => (phone.settings = s.id)}>{s.label}</button>
    {/each}
  </div>
  <div class="body">
    {#if shown}{@const C = shown.component}<C />{/if}
  </div>
</div>

<style>
  .settings { display: flex; flex-direction: column; height: 100%; }
  .bar { display: grid; grid-template-columns: repeat(var(--cols), minmax(0, 1fr)); gap: 2px; padding: var(--p-gap); flex: none; background: var(--panel); border-bottom: 1px solid var(--line); }
  .bar button { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-height: var(--p-row); padding: 0 3px; border: 0; border-radius: var(--p-radius); background: var(--panel-2); color: var(--text); font: inherit; font-size: var(--p-font-small); cursor: pointer; touch-action: manipulation; }
  .bar button.on { background: var(--accent); color: #fff; font-weight: 600; }
  .body { flex: 1; min-height: 0; overflow: hidden; }
</style>
