<script lang="ts">
  // The phone's Notebook tab: the Notebook and the Stations table behind one segmented switch. Each is made the first
  // time it is picked and then kept. (Fitting the notebook itself to one screen is a later step.)
  import type { Component } from 'svelte';
  import NotebookPanel from '../panels/NotebookPanel.svelte';
  import StationsPanel from '../panels/StationsPanel.svelte';
  import { phone, showPhoneTab, type NotebookView } from '../lib/phone.svelte';

  const VIEWS: { id: NotebookView; title: string; panel: Component }[] = [
    { id: 'notebook', title: 'Notebook', panel: NotebookPanel },
    { id: 'stations', title: 'Stations', panel: StationsPanel },
  ];
  let made = $state<Record<string, boolean>>({});
  $effect(() => {
    if (!made[phone.notebook]) made = { ...made, [phone.notebook]: true };
  });
</script>

<div class="nb">
  <div class="bar">
    <div class="seg" role="tablist" aria-label="Notebook or stations">
      {#each VIEWS as v (v.id)}
        <button role="tab" aria-selected={phone.notebook === v.id} class:on={phone.notebook === v.id} onclick={() => showPhoneTab(v.id)}>{v.title}</button>
      {/each}
    </div>
  </div>
  <div class="body">
    {#each VIEWS as v (v.id)}
      {#if made[v.id]}
        {@const P = v.panel}
        <div class="view" hidden={phone.notebook !== v.id}><P /></div>
      {/if}
    {/each}
  </div>
</div>

<style>
  .nb { display: flex; flex-direction: column; height: 100%; }
  .bar { display: flex; align-items: center; padding: var(--p-gap); flex: none; background: var(--panel); border-bottom: 1px solid var(--line); }
  .seg { display: flex; flex: 1; border: 1px solid var(--line); border-radius: var(--p-radius); overflow: hidden; }
  .seg button { flex: 1; min-height: 36px; border: 0; background: var(--panel-2); color: var(--text); font: inherit; cursor: pointer; touch-action: manipulation; }
  .seg button + button { border-left: 1px solid var(--line); }
  .seg button.on { background: var(--accent); color: #fff; font-weight: 600; }
  .body { flex: 1; min-height: 0; position: relative; }
  .view { position: absolute; inset: 0; display: flex; flex-direction: column; overflow: hidden; }
  .view[hidden] { display: none; }
  .view > :global(*) { flex: 1; min-height: 0; }
</style>
