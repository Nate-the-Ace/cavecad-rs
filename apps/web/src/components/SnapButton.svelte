<script lang="ts">
  // The Snap button of a view (F3 does the same): running snaps on or off, with a small arrow that opens the settings:
  // which modes run, tracking, the aperture, polar increments, and how the feedback looks.
  import { Magnet, ChevronDown } from '@lucide/svelte';
  import { snap, toggleSnap, setMode, setSetting, setPref, initSnap } from '../lib/snap.svelte';
  import { onMount } from 'svelte';

  onMount(() => void initSnap());

  const running = $derived(new Set(snap.settings.modes));
  const POLAR = [
    { label: '15°, 30°, 45°, 90°', value: [15, 30, 45, 90] },
    { label: '45°, 90°', value: [45, 90] },
    { label: '90° only', value: [90] },
    { label: '5°', value: [5] },
    { label: '10°', value: [10] },
  ];
  const polarKey = $derived(snap.settings.polar.join());
</script>

<span class="snap">
  <button class:on={snap.on} aria-pressed={snap.on} title="Object snap (F3): snap points to the ends, middles, centres and crossings of what is drawn" onclick={toggleSnap}><Magnet size={15} /> Snap</button>
  <button class="arrow" aria-label="Snap settings" aria-expanded={snap.settingsOpen} onclick={() => (snap.settingsOpen = !snap.settingsOpen)}><ChevronDown size={13} /></button>
  {#if snap.settingsOpen}
    <div class="pop" role="dialog" aria-label="Snap settings">
      {#each [['point', 'Points'], ['relational', 'Relations']] as [group, title]}
        <div class="head">{title}</div>
        {#each snap.catalog.filter((m) => m.group === group) as m (m.id)}
          <label><input type="checkbox" checked={running.has(m.id)} onchange={(e) => setMode(m.id, e.currentTarget.checked)} />{m.name}</label>
        {/each}
      {/each}
      <div class="head">Tracking</div>
      <label><input type="checkbox" checked={snap.settings.tracking} onchange={(e) => setSetting('tracking', e.currentTarget.checked)} />Track from acquired points</label>
      <label class="row">Angles
        <select value={polarKey} onchange={(e) => setSetting('polar', e.currentTarget.value.split(',').map(Number))}>
          {#each POLAR as p}<option value={p.value.join()}>{p.label}</option>{/each}
        </select>
      </label>
      <div class="head">Feedback</div>
      <label><input type="checkbox" checked={snap.prefs.marker} onchange={(e) => setPref('marker', e.currentTarget.checked)} />Marker</label>
      <label><input type="checkbox" checked={snap.prefs.tooltip} onchange={(e) => setPref('tooltip', e.currentTarget.checked)} />Tooltip</label>
      <label><input type="checkbox" checked={snap.prefs.magnet} onchange={(e) => setPref('magnet', e.currentTarget.checked)} />Magnet</label>
      <label class="row">Aperture {snap.settings.aperture}px
        <input type="range" min="4" max="30" step="1" value={snap.settings.aperture} oninput={(e) => setSetting('aperture', Number(e.currentTarget.value))} />
      </label>
    </div>
  {/if}
</span>

<style>
  .snap { position: relative; display: inline-flex; gap: 1px; }
  button { display: inline-flex; align-items: center; gap: 5px; }
  .arrow { padding: 0 3px; }
  .pop { position: absolute; top: calc(100% + 6px); left: 0; z-index: 30; width: 220px; max-height: 70vh; overflow: auto; padding: 6px 8px; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35); font-size: 12px; color: var(--text); }
  .head { margin-top: 6px; color: var(--muted); font-size: 10px; text-transform: uppercase; letter-spacing: 0.06em; }
  label { display: flex; align-items: center; gap: 6px; padding: 2px 0; }
  label.row { justify-content: space-between; }
  select, input[type='range'] { max-width: 120px; }
</style>
