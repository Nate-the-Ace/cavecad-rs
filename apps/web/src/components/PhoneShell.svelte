<script lang="ts">
  // The phone layout (an iPhone, either way up): one panel fills the screen, a single row of tabs along the bottom
  // switches between them, and a slim bar on top opens the Cave Shelf and says which cave and instrument are in use.
  // The panels are the same components the dock shows; each is made the first time its tab is opened and then kept,
  // so switching back finds it as it was. The ribbon is not shown: what the survey needs from it (connecting an
  // instrument, the shot settings) is in the Capture panel on a phone.
  import type { Component } from 'svelte';
  import { Activity, BookOpen, Crosshair, Library, Map as MapIcon, Mountain, Scissors, Table2 } from '@lucide/svelte';
  import { surface } from '../lib/surface.svelte';
  import MapPanel from '../panels/MapPanel.svelte';
  import ProfilePanel from '../panels/ProfilePanel.svelte';
  import SectionPanel from '../panels/SectionPanel.svelte';
  import StationsPanel from '../panels/StationsPanel.svelte';
  import NotebookPanel from '../panels/NotebookPanel.svelte';
  import SurveyPanel from '../panels/SurveyPanel.svelte';
  import { cave, unsaved } from '../lib/cave.svelte';
  import { shelf } from '../lib/shelf.svelte';
  import { survey } from '../lib/survey.svelte';
  import { PHONE_TABS, phone, showPhoneTab, type PhoneTab } from '../lib/phone.svelte';

  const TABS: Record<PhoneTab, { label: string; icon: Component; panel: Component }> = {
    survey: { label: 'Capture', icon: Crosshair, panel: SurveyPanel },
    map: { label: 'Map', icon: MapIcon, panel: MapPanel },
    section: { label: 'Section', icon: Scissors, panel: SectionPanel },
    notebook: { label: 'Notebook', icon: BookOpen, panel: NotebookPanel },
    stations: { label: 'Stations', icon: Table2, panel: StationsPanel },
    profile: { label: 'Profile', icon: Activity, panel: ProfilePanel },
  };

  // made once opened, then kept
  let made = $state<Record<string, boolean>>({});
  $effect(() => {
    if (!made[phone.tab]) made = { ...made, [phone.tab]: true };
  });

  const linked = $derived(survey.status === 'connected');
  const dirty = $derived(unsaved());
</script>

<div class="phone">
  <header class="top">
    <button class="caves" onclick={() => (shelf.open = true)} aria-label="Caves: open, start or switch a cave"><Library size={18} /> Caves</button>
    <span class="name" title={cave.name}>{cave.hasCave ? cave.name || 'Untitled cave' : 'No cave open'}{#if dirty}<span class="dot" title="Unsaved changes"> •</span>{/if}</span>
    {#if cave.hasCave}<button class="caves icon" onclick={() => (surface.dialog = true)} aria-label="Surface Data: where the cave is, aerial photo and ground"><Mountain size={18} /></button>{/if}
    <span class="chip" class:on={linked} role="status">{linked ? survey.device || 'Connected' : 'No instrument'}</span>
  </header>

  <main class="panes">
    {#each PHONE_TABS as id (id)}
      {#if made[id]}
        {@const P = TABS[id].panel}
        <section class="pane" class:shown={phone.tab === id} aria-label={TABS[id].label} hidden={phone.tab !== id}><P /></section>
      {/if}
    {/each}
  </main>

  <nav class="tabs" aria-label="Panels">
    {#each PHONE_TABS as id (id)}
      {@const T = TABS[id]}
      <button class:sel={phone.tab === id} aria-current={phone.tab === id ? 'page' : undefined} onclick={() => showPhoneTab(id)}>
        <T.icon size={20} />
        <span>{T.label}</span>
      </button>
    {/each}
  </nav>
</div>

<style>
  .phone {
    display: flex; flex-direction: column; flex: 1; width: 100%; min-width: 0; height: 100%; background: var(--bg, var(--panel));
    padding-left: env(safe-area-inset-left); padding-right: env(safe-area-inset-right);
  }
  .top {
    display: flex; align-items: center; gap: 8px; padding: 6px 10px; padding-top: calc(6px + env(safe-area-inset-top));
    background: var(--panel); border-bottom: 1px solid var(--line); min-height: 40px;
  }
  .caves { display: inline-flex; align-items: center; gap: 6px; font: inherit; font-size: 13px; padding: 6px 10px; min-height: 36px;
    border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); }
  .caves.icon { padding: 6px 8px; }
  .name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-weight: 600; font-size: 14px; color: var(--text); }
  .dot { color: var(--warn); }
  .chip { padding: 3px 10px; border-radius: 999px; border: 1px solid var(--line); color: var(--muted); font-size: 12px; white-space: nowrap; max-width: 40%; overflow: hidden; text-overflow: ellipsis; }
  .chip.on { border-color: var(--accent); color: var(--text); }
  .panes { flex: 1; min-height: 0; position: relative; }
  .pane { position: absolute; inset: 0; overflow: hidden; }
  .pane[hidden] { display: none; }
  .tabs {
    display: grid; grid-template-columns: repeat(6, 1fr); border-top: 1px solid var(--line); background: var(--panel);
    padding-bottom: env(safe-area-inset-bottom);
  }
  .tabs button {
    display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 2px; min-height: 52px;
    padding: 4px 0; border: 0; background: none; color: var(--muted); font: inherit; font-size: 10.5px; touch-action: manipulation;
  }
  .tabs button.sel { color: var(--accent); font-weight: 600; }
</style>
