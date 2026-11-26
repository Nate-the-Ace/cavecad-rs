<script lang="ts">
  // What snaps: which modes run, tracking, the angles, how the feedback looks and the aperture. The same settings the
  // Snap button's arrow opens in a drawing view, here for Settings (the views keep only the on/off toggle).
  import { snap, setMode, setSetting, setPref, initSnap } from '../lib/snap.svelte';
  import { onMount } from 'svelte';
  import SnapGlyph from './SnapGlyph.svelte';
  import type { SnapMode } from '../lib/snap-types';

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
    <div class="tiles" role="group" aria-label={title}>
      {#each snap.catalog.filter((m) => m.group === group) as m (m.id)}
        <button class="tile" class:on={running.has(m.id)} aria-pressed={running.has(m.id)} title={m.name} onclick={() => setMode(m.id, !running.has(m.id))}>
          <SnapGlyph mode={m.id as SnapMode} color={running.has(m.id) ? 'var(--accent)' : 'var(--text)'} />
          <span class="nm">{m.name}</span>
        </button>
      {/each}
    </div>
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
  .tiles { display: grid; grid-template-columns: repeat(auto-fill, minmax(68px, 1fr)); gap: 6px; }
  .tile { display: flex; flex-direction: column; align-items: center; justify-content: flex-start; gap: 2px; min-height: 64px; padding: 6px 2px 4px; border: 1px solid var(--line); border-radius: 8px; background: var(--panel-2); color: var(--text); font: inherit; touch-action: manipulation; cursor: pointer; }
  .tile.on { background: var(--accent-bg); border-color: var(--accent); }
  .tile .nm { font-size: 10.5px; line-height: 1.1; text-align: center; overflow-wrap: anywhere; color: var(--muted); }
  .tile.on .nm { color: var(--text); }
  .snapset { display: flex; flex-direction: column; gap: 2px; font-size: 13px; color: var(--text); }
  .head { margin-top: 6px; color: var(--muted); font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; }
  label { display: flex; align-items: center; gap: 8px; padding: 3px 0; min-height: 30px; }
  label.row { justify-content: space-between; }
  label input[type='checkbox'] { width: 20px; height: 20px; margin: 0; flex: none; }
  select, input[type='range'] { max-width: 160px; }
</style>
