<script lang="ts">
  // The phone Survey screen's More sheet: what the desktop Capture panel offers beyond the shot (the trip the shots go
  // to, the X1's laser, starting a sweep, a feature shot, stepping back a station) and the session's shots, the one
  // list here that scrolls. A sheet from the bottom, closed by Done or a tap above it.
  import { untrack } from 'svelte';
  import { ChevronLeft, Flashlight, FlashlightOff, Radar, ScanLine, Star } from '@lucide/svelte';
  import { cave } from '../lib/cave.svelte';
  import { engine, loadEngine } from '../lib/engine';
  import type { TripChoice } from '../lib/notebook';
  import { angleText } from '../lib/notebook';
  import { formatLength } from '../lib/units';
  import * as live from '../lib/survey.svelte';
  import { survey } from '../lib/survey.svelte';
  import { isLinked, sweepName } from '../lib/phone-survey';

  let { onclose, onfeature }: { onclose: () => void; onfeature: () => void } = $props();

  let ready = $state(false);
  let trips = $state<TripChoice[]>([]);
  loadEngine().then(() => (ready = true));
  $effect(() => {
    void cave.version;
    void ready;
    untrack(() => (trips = ready ? engine.notebookTrips() : []));
  });

  const linked = $derived(isLinked(survey.status));
  const manual = $derived(survey.standard === 'manual');
  const busy = $derived(!!survey.sweep || !!survey.waitingForDevice);
  const last = $derived(survey.lastSweep);
  const dist = (m: number) => formatLength(m, cave.units, 2);
  const act = (f: () => void) => () => {
    f();
    onclose();
  };
</script>

<div class="veil" role="presentation" onclick={onclose}></div>
<div class="sheet" role="dialog" aria-label="More survey controls">
  <header><b>Survey</b><button class="done" onclick={onclose}>Done</button></header>
  <label class="row"><span class="lab">Trip</span>
    <select value={cave.trip} onchange={(e) => (cave.trip = e.currentTarget.value)} aria-label="Trip">
      {#each trips as t (t.id)}<option value={t.id}>{t.name || 'Untitled trip'}</option>{/each}
    </select>
  </label>
  {#if survey.canLaser}
    <div class="row">
      <button onclick={() => live.laser(true)}><Flashlight size={18} /> Laser on</button>
      <button onclick={() => live.laser(false)}><FlashlightOff size={18} /> Laser off</button>
    </div>
  {/if}
  <div class="row">
    <button onclick={act(() => live.startSweep('lrud'))} disabled={!linked || busy}><ScanLine size={18} /> {sweepName('lrud', manual)}</button>
    <button onclick={act(() => live.startSweep('room'))} disabled={!linked || busy}><Radar size={18} /> {sweepName('room', manual)}</button>
  </div>
  {#if last}
    <p class="line" class:warn={!last.check.complete}>Last: {sweepName(last.check.kind, manual)} at {last.station}, {last.check.standard === 'digital' ? `${Math.floor(last.check.coverage)}%` : `${last.check.shots} shot${last.check.shots === 1 ? '' : 's'}`}{last.check.complete ? '' : ', not complete'}</p>
  {/if}
  <div class="row">
    <button onclick={act(onfeature)} disabled={!survey.canTakeShot || !!survey.feature}><Star size={18} /> Feature shot</button>
    <button onclick={act(live.previousStation)} disabled={survey.state.awaiting !== null}><ChevronLeft size={18} /> Previous station</button>
  </div>
  <div class="log" aria-label="Shots this session">
    {#each [...survey.log].reverse() as e (e.n)}
      <div class="entry" class:flag={e.warnings.length > 0} title={e.warnings.join(' ')}>
        <b>{e.label}</b>
        <span>{dist(e.reading.distance)} · {angleText(+e.reading.azimuth.toFixed(1))}° · {angleText(+e.reading.inclination.toFixed(1))}°</span>
      </div>
    {:else}
      <p class="line">No shots yet</p>
    {/each}
  </div>
</div>

<style>
  .veil { position: fixed; inset: 0; z-index: 50; background: rgba(0, 0, 0, 0.4); }
  .sheet {
    position: fixed; left: 0; right: 0; bottom: 0; z-index: 51; max-height: 75vh; display: flex; flex-direction: column; gap: var(--p-gap);
    padding: var(--p-pad) var(--p-pad) calc(var(--p-pad) + env(safe-area-inset-bottom)); box-sizing: border-box;
    background: var(--panel); color: var(--text); border-top: 1px solid var(--line); border-radius: 12px 12px 0 0;
  }
  header { display: flex; align-items: center; justify-content: space-between; padding-left: 4px; flex: none; }
  .row { display: flex; gap: var(--p-gap); align-items: center; flex: none; }
  .row > * { flex: 1 1 0; min-width: 0; }
  .lab { flex: 0 0 48px; color: var(--muted); font-size: var(--p-font-small); }
  button, select { font: inherit; min-height: var(--p-row); padding: 0 8px; border: 1px solid var(--line); border-radius: var(--p-radius); background: var(--panel-2); color: var(--text); box-sizing: border-box; }
  select { font-size: var(--p-font-input); }
  button { display: inline-flex; align-items: center; justify-content: center; gap: 4px; cursor: pointer; touch-action: manipulation; white-space: nowrap; overflow: hidden; }
  button:disabled { opacity: 0.45; cursor: default; }
  .done { flex: none; min-width: 72px; background: var(--accent); border-color: var(--accent); color: var(--on-accent, #fff); font-weight: 600; }
  .line { margin: 0; font-size: var(--p-font-small); color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex: none; }
  .line.warn { color: var(--warn); }
  .log { flex: 1 1 auto; min-height: 60px; max-height: 40vh; overflow-y: auto; border-top: 1px solid var(--line); touch-action: pan-y; }
  .entry { display: flex; justify-content: space-between; gap: 8px; padding: 4px 2px; border-bottom: 1px solid var(--line); font-size: var(--p-font-small); }
  .entry span { color: var(--muted); font-variant-numeric: tabular-nums; }
  .entry.flag b { color: var(--warn); }
</style>
