<script lang="ts">
  // The phone's Notebook tab: the Notebook and the Stations table, each on one screen (its panel's `compact` layout:
  // a two-row header, one bounded list). The two-segment switch is not a row of its own: it is handed to the panels as
  // `lead` and sits at the left of their first header row. Each is made the first time it is picked and then kept.
  import type { Component } from 'svelte';
  import { BookOpen, Table2 } from '@lucide/svelte';
  import NotebookPanel from '../panels/NotebookPanel.svelte';
  import StationsPanel from '../panels/StationsPanel.svelte';
  import { phone, showPhoneTab, type NotebookView } from '../lib/phone.svelte';

  const VIEWS: { id: NotebookView; title: string; panel: Component<any> }[] = [
    { id: 'notebook', title: 'Notebook', panel: NotebookPanel },
    { id: 'stations', title: 'Stations', panel: StationsPanel },
  ];
  let made = $state<Record<string, boolean>>({});
  $effect(() => {
    if (!made[phone.notebook]) made = { ...made, [phone.notebook]: true };
  });
</script>

{#snippet lead()}
  <div class="seg" role="tablist" aria-label="Notebook or stations">
    {#each VIEWS as v (v.id)}
      <button role="tab" aria-label={v.title} title={v.title} aria-selected={phone.notebook === v.id} class:on={phone.notebook === v.id} onclick={() => showPhoneTab(v.id)}>
        {#if v.id === 'notebook'}<BookOpen size={18} />{:else}<Table2 size={18} />{/if}
      </button>
    {/each}
  </div>
{/snippet}

<div class="nb">
  {#each VIEWS as v (v.id)}
    {#if made[v.id]}
      {@const P = v.panel}
      <div class="view" hidden={phone.notebook !== v.id}><P compact {lead} /></div>
    {/if}
  {/each}
</div>

<style>
  .nb { position: relative; height: 100%; }
  .view { position: absolute; inset: 0; display: flex; flex-direction: column; overflow: hidden; }
  .view[hidden] { display: none; }
  .view > :global(*) { flex: 1; min-height: 0; }
  :global(.seg) { flex: none; display: flex; border: 1px solid var(--line); border-radius: var(--p-radius); overflow: hidden; }
  :global(.seg) button { width: 40px; height: 40px; padding: 0; display: inline-grid; place-items: center; border: 0; background: var(--panel-2); color: var(--text); cursor: pointer; touch-action: manipulation; }
  :global(.seg) button + button { border-left: 1px solid var(--line); }
  :global(.seg) button.on { background: var(--accent); color: #fff; }
</style>
