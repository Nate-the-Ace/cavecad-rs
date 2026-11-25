<script lang="ts">
  // The phone's Sketch tab: the app's own Map, Profile and Cross Section panels (the ones the desktop dock mounts)
  // behind one segmented switch, under ONE bar: [Map|Profile|Section] [tools] [Surface Data] [undo redo].
  // The undo pair is the active view's own scope; the panels' own pairs are hidden (.canvas rule below) so the phone
  // shows just one. Each view is made the first time it is picked and then kept (hidden), so the 3D Map keeps its camera.
  // `.tools` is the slot for the drawing-type dropdown and Symbols button (they share the bar's line); `.overlay` is a
  // thin region over the top of the canvas kept free for a live-shot strip. Neither holds anything yet.
  import type { Component } from 'svelte';
  import { Mountain } from '@lucide/svelte';
  import PanelUndo from './PanelUndo.svelte';
  import MapPanel from '../panels/MapPanel.svelte';
  import ProfilePanel from '../panels/ProfilePanel.svelte';
  import SectionPanel from '../panels/SectionPanel.svelte';
  import { surface } from '../lib/surface.svelte';
  import type { Scope } from '../lib/engine';
  import { phone, showPhoneTab, type SketchView } from '../lib/phone.svelte';

  const VIEWS: { id: SketchView & Scope; title: string; label: string; panel: Component }[] = [
    { id: 'map', title: 'Map', label: 'on the map', panel: MapPanel },
    { id: 'profile', title: 'Profile', label: 'in the profile', panel: ProfilePanel },
    { id: 'section', title: 'Section', label: 'in this cross section', panel: SectionPanel },
  ];
  const current = $derived(VIEWS.find((v) => v.id === phone.sketch) ?? VIEWS[0]);

  let made = $state<Record<string, boolean>>({});
  $effect(() => {
    if (!made[current.id]) made = { ...made, [current.id]: true };
  });
</script>

<div class="sketch">
  <div class="bar">
    <div class="seg" role="tablist" aria-label="View">
      {#each VIEWS as v (v.id)}
        <button role="tab" aria-selected={current.id === v.id} class:on={current.id === v.id} onclick={() => showPhoneTab(v.id)}>{v.title}</button>
      {/each}
    </div>
    <div class="tools"></div>
    <button class="icon" onclick={() => (surface.dialog = true)} aria-label="Surface Data" title="Surface Data"><Mountain size={18} /></button>
    {#key current.id}<div class="undo"><PanelUndo scope={current.id} label={current.label} /></div>{/key}
  </div>
  <div class="canvas">
    {#each VIEWS as v (v.id)}
      {#if made[v.id]}
        {@const P = v.panel}
        <div class="view" hidden={current.id !== v.id}><P /></div>
      {/if}
    {/each}
    <div class="overlay"></div>
  </div>
</div>

<style>
  .sketch { display: flex; flex-direction: column; height: 100%; }
  .bar { display: flex; align-items: center; gap: var(--p-gap); padding: 0 var(--p-gap); min-height: var(--p-hit); flex: none; background: var(--panel); border-bottom: 1px solid var(--line); }
  .seg { display: flex; flex: none; border: 1px solid var(--line); border-radius: var(--p-radius); overflow: hidden; }
  .seg button { min-height: 36px; padding: 0 9px; border: 0; background: var(--panel-2); color: var(--text); font: inherit; font-size: var(--p-font-small); cursor: pointer; touch-action: manipulation; }
  .seg button + button { border-left: 1px solid var(--line); }
  .seg button.on { background: var(--accent); color: #fff; font-weight: 600; }
  .tools { flex: 1; min-width: 0; display: flex; align-items: center; gap: var(--p-gap); }
  .icon, .undo :global(button) {
    min-width: var(--p-hit); min-height: var(--p-hit); padding: 0; border: 0; border-radius: var(--p-radius); background: none; color: var(--text);
    display: inline-flex; align-items: center; justify-content: center; cursor: pointer; touch-action: manipulation;
  }
  .undo { display: flex; flex: none; }
  .undo :global(svg) { width: 20px; height: 20px; }
  .undo :global(button:disabled) { opacity: 0.35; }
  .canvas { flex: 1; min-height: 0; position: relative; }
  .view { position: absolute; inset: 0; display: flex; flex-direction: column; overflow: hidden; }
  .view[hidden] { display: none; }
  .view > :global(*) { flex: 1; min-height: 0; }
  /* kept free for a thin live-shot strip later; takes no touches until it has something in it */
  .overlay { position: absolute; left: 0; right: 0; top: 0; pointer-events: none; }
  /* the panels' own undo/redo duplicate the pair in the bar */
  .canvas :global(button[aria-label^='Undo']), .canvas :global(button[aria-label^='Redo']) { display: none; }
</style>
