<script lang="ts">
  // What snaps: which modes run, tracking, the angles, how the feedback looks and the aperture. The same settings the
  // Snap button's arrow opens in a drawing view, here for Settings (the views keep only the on/off toggle).
  import { snap, setMode, setSetting, setPref, initSnap } from '../lib/snap.svelte';
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

<div class="snapset">
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

<style>
  .snapset { display: flex; flex-direction: column; gap: 2px; font-size: 13px; color: var(--text); }
  .head { margin-top: 6px; color: var(--muted); font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; }
  label { display: flex; align-items: center; gap: 8px; padding: 3px 0; min-height: 30px; }
  label.row { justify-content: space-between; }
  label input[type='checkbox'] { width: 20px; height: 20px; margin: 0; flex: none; }
  select, input[type='range'] { max-width: 160px; }
</style>
