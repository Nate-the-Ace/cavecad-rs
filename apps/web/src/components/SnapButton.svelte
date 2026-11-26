<script lang="ts">
  // The Snap button of a view (F3 does the same): running snaps on or off, with a small arrow that opens the settings:
  // which modes run, tracking, the aperture, polar increments, and how the feedback looks.
  import { Magnet, ChevronDown } from '@lucide/svelte';
  import { snap, toggleSnap, initSnap } from '../lib/snap.svelte';
  import { onMount } from 'svelte';
  import SnapSettings from './SnapSettings.svelte';

  /** Only the on/off toggle, no arrow: the settings live in Settings (Sketch). */
  let { toggleOnly = false }: { toggleOnly?: boolean } = $props();
  onMount(() => void initSnap());

</script>

<span class="snap">
  <button class:on={snap.on} aria-pressed={snap.on} title="Object snap (F3): snap points to the ends, middles, centres and crossings of what is drawn" onclick={toggleSnap}><Magnet size={15} /> Snap {snap.on ? 'on' : 'off'}</button>
  {#if !toggleOnly}<button class="arrow" aria-label="Snap settings" aria-expanded={snap.settingsOpen} onclick={() => (snap.settingsOpen = !snap.settingsOpen)}><ChevronDown size={13} /></button>{/if}
  {#if snap.settingsOpen && !toggleOnly}
    <div class="pop" role="dialog" aria-label="Snap settings">
      <SnapSettings />
    </div>
  {/if}
</span>

<style>
  .snap { position: relative; display: inline-flex; gap: 1px; }
  button { display: inline-flex; align-items: center; gap: 5px; }
  /* a toggle you can read at a glance: solid accent when snapping is on, plain and dimmed when it is off */
  .snap > button:not(.arrow) { border: 1px solid var(--line); background: var(--panel-2); color: var(--muted); }
  .snap > button.on:not(.arrow) { background: var(--accent); border-color: var(--accent); color: var(--on-accent, #fff); font-weight: 600; }
  .arrow { padding: 0 3px; }
  .pop { position: absolute; top: calc(100% + 6px); left: 0; z-index: 30; width: 220px; max-height: 70vh; overflow: auto; padding: 6px 8px; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35); font-size: 12px; color: var(--text); }
</style>
