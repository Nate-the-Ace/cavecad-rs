<script lang="ts">
  // Guided calibration: follows the X1's calibration shots (it sends each one as it is taken) and says where to point
  // next. The X1 does the calibration itself (M, Calibration on the X1); this only guides, like the X1's two circles
  // but in words. The rules are Rust: cave-instrument calibrate.rs.
  import { Check, RotateCcw, X } from '@lucide/svelte';
  import type { CalibrationView } from '../lib/survey-types';
  import * as live from '../lib/survey.svelte';
  import { survey } from '../lib/survey.svelte';
  import { phone } from '../lib/phone.svelte';

  let { guide }: { guide: CalibrationView } = $props();

  const R = 46;
  /** A direction on one of the two circles (north at the top, clockwise, the rim level, the centre straight up or down). */
  const at = (az: number, inc: number, up: boolean) => {
    const r = ((90 - (up ? inc : -inc)) / 90) * R;
    const a = (az * Math.PI) / 180;
    return { x: 50 + r * Math.sin(a), y: 50 - r * Math.cos(a) };
  };
  const onHalf = (inc: number, up: boolean) => (up ? inc >= -0.5 : inc <= 0.5);
</script>

<div class="guide" role="region" aria-label="Guided calibration">
  <div class="head">
    <b>Guided calibration</b>
    <span class="muted">{guide.covered} of {guide.targets.length} directions · {guide.groups.length} group{guide.groups.length === 1 ? '' : 's'}</span>
  </div>
  <p class="say" class:done={guide.done}>{guide.say}</p>
  <div class="dots" aria-label="Shots in this group">
    {#each [0, 1, 2, 3] as k (k)}<span class:on={k < guide.current_shots}></span>{/each}
    <small>this group</small>
  </div>
  <div class="circles">
    {#each [true, false] as up (up)}
      <svg viewBox="0 0 100 108" role="img" aria-label={up ? 'Upper half' : 'Lower half'}>
        <circle cx="50" cy="50" r={R} class="rim" />
        <circle cx="50" cy="50" r={R / 2} class="mid" />
        <text x="50" y="7.5" class="n">N</text>
        {#each guide.targets.filter((t) => onHalf(t.inclination, up)) as t (t.label)}
          {@const p = at(t.azimuth, t.inclination, up)}
          <circle cx={p.x} cy={p.y} r="5.2" class="t" class:cov={t.covered} class:next={guide.next?.label === t.label} />
        {/each}
        {#each guide.groups.filter((g) => onHalf(g.inclination, up)) as g, i (i)}
          {@const p = at(g.azimuth, g.inclination, up)}
          <text x={p.x} y={p.y + 2.6} class="x">×</text>
        {/each}
        {#if guide.current && onHalf(guide.current.inclination, up)}
          {@const p = at(guide.current.azimuth, guide.current.inclination, up)}
          <text x={p.x} y={p.y + 2.8} class="plus">+</text>
        {/if}
        <text x="50" y="105" class="cap">{up ? 'Up' : 'Down'}</text>
      </svg>
    {/each}
  </div>
  <ul class="list">
    {#each guide.targets as t (t.label)}
      <li class:cov={t.covered} class:next={guide.next?.label === t.label}>{#if t.covered}<Check size={13} />{:else}<span class="box"></span>{/if}{t.label}</li>
    {/each}
  </ul>
  <div class="row">
    {#if survey.kind === 'simulator' && !phone.on}<button onclick={live.simCalibrationShot}>Simulated shot</button>{/if}
    <button onclick={live.resetCalibrationGroup} title="Press this when you reset the group on the X1 (its > button)"><RotateCcw size={16} /> Group reset on X1</button>
    <button onclick={live.stopCalibration}><X size={16} /> Close guide</button>
  </div>
  <p class="muted small">On the X1: M, then Calibration. Shoot with its DIST button. When {guide.can_compute ? 'done' : 'you have 8 or more groups'}: M computes, M applies.</p>
</div>

<style>
  .guide { display: flex; flex-direction: column; gap: 8px; padding: 10px; border: 1px solid var(--accent); border-radius: var(--radius); }
  .head { display: flex; justify-content: space-between; gap: 8px; flex-wrap: wrap; }
  .muted { color: var(--muted); font-size: 12px; }
  .small { margin: 0; }
  .say { margin: 0; font-size: 16px; font-weight: 700; color: var(--warn); }
  .say.done { color: var(--accent); }
  .dots { display: flex; align-items: center; gap: 6px; }
  .dots span { width: 14px; height: 14px; border-radius: 50%; border: 2px solid var(--accent); }
  .dots span.on { background: var(--accent); }
  .dots small { color: var(--muted); margin-left: 4px; }
  .circles { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; max-width: 360px; }
  svg { width: 100%; height: auto; }
  .rim { fill: var(--panel-2); stroke: var(--line); }
  .mid { fill: none; stroke: var(--line); stroke-dasharray: 2 2; }
  .t { fill: none; stroke: var(--muted); stroke-width: 1.2; }
  .t.cov { fill: var(--accent); stroke: var(--accent); opacity: 0.85; }
  .t.next { stroke: var(--warn); stroke-width: 2.4; }
  .x { font-size: 8px; text-anchor: middle; fill: var(--text); font-weight: 700; }
  .plus { font-size: 10px; text-anchor: middle; fill: var(--warn); font-weight: 700; }
  .n, .cap { font-size: 7px; text-anchor: middle; fill: var(--muted); }
  .list { list-style: none; margin: 0; padding: 0; display: grid; grid-template-columns: 1fr 1fr; gap: 2px 10px; font-size: 12px; }
  .list li { display: flex; align-items: center; gap: 4px; color: var(--muted); }
  .list li.cov { color: var(--text); }
  .list li.next { color: var(--warn); font-weight: 700; }
  .box { width: 11px; height: 11px; border: 1px solid var(--muted); border-radius: 2px; display: inline-block; }
  .row { display: flex; flex-wrap: wrap; gap: 8px; }
  .row button { flex: 1; min-height: 40px; }
</style>
