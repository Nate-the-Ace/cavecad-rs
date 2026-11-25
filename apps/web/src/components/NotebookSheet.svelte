<script lang="ts">
  // A small bottom sheet for the phone Notebook ("More" fields, a station's note). Only the Notebook uses it. It
  // rides above the on-screen keyboard (visualViewport) so the field being typed in stays in sight.
  import type { Snippet } from 'svelte';
  import { X } from '@lucide/svelte';

  let { open, title, onclose, children }: { open: boolean; title: string; onclose: () => void; children: Snippet } = $props();
  let lift = $state(0);

  $effect(() => {
    if (!open) return;
    const vv = window.visualViewport;
    if (!vv) return;
    const sync = () => (lift = Math.max(0, Math.round(window.innerHeight - vv.height - vv.offsetTop)));
    sync();
    vv.addEventListener('resize', sync);
    vv.addEventListener('scroll', sync);
    return () => {
      vv.removeEventListener('resize', sync);
      vv.removeEventListener('scroll', sync);
      lift = 0;
    };
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions: the close button is the keyboard route -->
  <div class="back" onclick={onclose}></div>
  <div class="sheet" role="dialog" aria-label={title} style:bottom="{lift}px">
    <div class="head"><b>{title}</b><button class="x" aria-label="Close" onclick={onclose}><X size={18} /></button></div>
    <div class="body">{@render children()}</div>
  </div>
{/if}

<style>
  .back { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.35); z-index: 60; }
  .sheet { position: fixed; left: 0; right: 0; z-index: 61; max-height: 75dvh; display: flex; flex-direction: column; background: var(--panel); color: var(--text); border-top: 1px solid var(--line); border-radius: var(--p-radius, 8px) var(--p-radius, 8px) 0 0; padding: 0 var(--p-pad, 6px) env(safe-area-inset-bottom); box-sizing: border-box; }
  .head { display: flex; align-items: center; justify-content: space-between; min-height: 40px; }
  .x { width: 40px; height: 40px; display: inline-grid; place-items: center; border: 0; background: none; color: var(--text); touch-action: manipulation; }
  .body { overflow-y: auto; padding-bottom: var(--p-pad, 6px); display: flex; flex-direction: column; gap: var(--p-gap, 4px); }
</style>
