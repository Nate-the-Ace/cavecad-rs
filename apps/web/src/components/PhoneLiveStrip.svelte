<script lang="ts">
  // The phone Sketch screen's live shot strip: the latest shot's tape / compass / clino, the connection dot and a Take
  // shot button over the bottom of the canvas (an overlay: the canvas keeps its full height). Tap the numbers to fold it
  // to a small dot at the corner. With no instrument it is just the dot and Connect; held readings (Write at once off)
  // show Accept / Discard. No words: the Survey tab has the detail.
  import { untrack } from 'svelte';
  import { Check, Crosshair, Star, X } from '@lucide/svelte';
  import { cave } from '../lib/cave.svelte';
  import { engine, loadEngine } from '../lib/engine';
  import { angleText } from '../lib/notebook';
  import { formatLength } from '../lib/units';
  import * as live from '../lib/survey.svelte';
  import { survey } from '../lib/survey.svelte';
  import { showSettings } from '../lib/phone.svelte';
  import { atStationOnly, linkLabel } from '../lib/phone-survey';
  import { splitUnit, stripModel } from '../lib/phone-livestrip';

  const KEY = 'cavecad.phone.liveStrip';
  const read = (): boolean => {
    try {
      return localStorage.getItem(KEY) === 'collapsed';
    } catch {
      return false;
    }
  };
  let collapsed = $state(read());
  const setCollapsed = (v: boolean) => {
    collapsed = v;
    try {
      localStorage.setItem(KEY, v ? 'collapsed' : 'open');
    } catch {
      /* just not remembered */
    }
  };

  // The Survey tab sets the session up for the open trip, but only once it has been mounted: do the same here.
  let ready = $state(false);
  loadEngine().then(() => (ready = true));
  $effect(() => {
    void cave.trip;
    void cave.opened;
    void ready;
    untrack(() => ready && live.syncToTrip());
  });

  const m = $derived(
    stripModel({
      status: survey.status,
      collapsed,
      held: survey.held.length,
      aimsFirst: live.aimsFirst(),
      feature: !!survey.feature,
      canTakeShot: survey.canTakeShot,
      awaiting: !!survey.state.awaiting,
      hasTo: !!survey.state.to.trim(),
      burst: survey.burst,
      aiming: survey.aiming,
      message: survey.message,
      warnings: survey.warnings.length,
      preferred: survey.shotType,
      atOnly: atStationOnly({ awaiting: !!survey.state.awaiting, mode: survey.state.mode, sweeping: !!survey.sweep || !!survey.waitingForDevice }),
    }),
  );
  const last = $derived(survey.last);
  const tape = $derived(last ? splitUnit(formatLength(last.distance, cave.units, 2)) : null);
  const problemLabel = $derived(m.problem === 'problem' ? 'Something went wrong: see the Survey tab' : 'A caution: see the Survey tab');
  const dotLabel = $derived(linkLabel(survey.status, survey.device));

  function take() {
    if (!m.shot) return;
    if (m.shot.kind === 'feature') live.shootFeature();
    else if (m.shot.press) live.pressShot(m.shot.press);
    else live.pressShot();
  }
  const stop = (e: Event) => e.stopPropagation();
</script>

{#if m.view === 'collapsed'}
  <button class="fold" onpointerdown={stop} onclick={() => setCollapsed(false)} aria-label="Show the live shot strip" title={dotLabel}>
    <i class="dot {m.dot}"></i>
    {#if m.problem}<i class="flag {m.problem}"></i>{/if}
  </button>
{:else}
  <div class="live" role="group" aria-label="Live shot" onpointerdown={stop} onpointermove={stop} onpointerup={stop}>
    {#if m.dotOpensSettings}
      <button class="dotbtn" onclick={() => showSettings('connection')} aria-label="{dotLabel}: open connection settings"><i class="dot {m.dot}"></i></button>
    {:else}
      <span class="dotbtn" role="img" aria-label={dotLabel}><i class="dot {m.dot}"></i></span>
    {/if}
    {#if m.view === 'connect'}
      <span class="grow"></span>
      <button class="word" onclick={() => (m.connectLabel === 'Stop' ? live.disconnect() : void live.connect('auto'))}>{m.connectLabel}</button>
      <button class="hide" onclick={() => setCollapsed(true)} aria-label="Fold the live shot strip"><X size={16} /></button>
    {:else if m.view === 'held'}
      <span class="grow"></span>
      <button class="word go" onclick={live.accept}><Check size={16} /> Accept</button>
      <button class="word" onclick={live.discard}><X size={16} /> Discard</button>
    {:else}
      <button class="nums" onclick={() => setCollapsed(true)} aria-label="Fold the live shot strip">
        <span class="n">{tape ? tape.value : '–'}<small>{tape?.unit ?? 'm'}</small></span>
        <span class="n">{last ? angleText(+last.azimuth.toFixed(1)) : '–'}<small>deg</small></span>
        <span class="n">{last ? angleText(+last.inclination.toFixed(1)) : '–'}<small>deg</small></span>
      </button>
    {/if}
    {#if m.problem}<i class="flag {m.problem}" role="img" aria-label={problemLabel}></i>{/if}
    {#if m.shot}
      <button class="take" class:armed={m.shot.armed} onclick={take} disabled={!m.shot.enabled} aria-label={m.shot.kind === 'feature' ? 'Shoot the feature' : m.shot.armed ? 'Shoot' : 'Take shot'}>
        {#if m.shot.kind === 'feature'}<Star size={22} />{:else}<Crosshair size={22} />{/if}
      </button>
    {/if}
  </div>
{/if}

<style>
  .live {
    position: absolute; left: var(--p-pad); right: var(--p-pad); bottom: var(--p-pad); height: 40px; z-index: 30;
    display: flex; align-items: center; gap: var(--p-gap); box-sizing: border-box; padding: 0 0 0 2px;
    border: 1px solid var(--line); border-radius: 12px; overflow: hidden;
    background: color-mix(in srgb, var(--panel) 92%, transparent); backdrop-filter: blur(6px); touch-action: manipulation;
  }
  button { font: inherit; color: var(--text); touch-action: manipulation; cursor: pointer; }
  .dotbtn { flex: none; width: 40px; height: 40px; padding: 0; border: 0; background: none; display: inline-flex; align-items: center; justify-content: center; }
  .dot { display: block; width: 12px; height: 12px; border-radius: 50%; background: var(--muted, #888); }
  .dot.green { background: #2fb344; }
  .dot.amber { background: #f0a020; }
  .dot.grey { background: #8a8f98; }
  .grow { flex: 1; }
  .nums { flex: 1; min-width: 0; height: 40px; padding: 0; border: 0; background: none; display: flex; align-items: center; justify-content: space-around; gap: var(--p-gap); }
  .n { font: 700 15px/1 ui-monospace, SFMono-Regular, Menlo, monospace; font-variant-numeric: tabular-nums; white-space: nowrap; }
  .n small { margin-left: 1px; font-size: 9px; font-weight: 500; opacity: 0.65; }
  .word { min-height: 40px; min-width: 64px; padding: 0 10px; border: 0; border-radius: var(--p-radius); background: none; font-size: var(--p-font); font-weight: 600; display: inline-flex; align-items: center; justify-content: center; gap: 4px; }
  .word.go { color: var(--accent); }
  .hide { flex: none; width: 40px; height: 40px; padding: 0; border: 0; background: none; display: inline-flex; align-items: center; justify-content: center; opacity: 0.7; }
  .flag { flex: none; width: 8px; height: 8px; border-radius: 50%; }
  .flag.problem { background: #e5484d; }
  .flag.caution { background: #f0a020; }
  .take { flex: none; width: 48px; height: 40px; padding: 0; border: 0; border-radius: 0 11px 11px 0; background: var(--accent); color: #fff; display: inline-flex; align-items: center; justify-content: center; }
  .take.armed { background: #e5484d; }
  .take:disabled { opacity: 0.4; }
  .fold { position: absolute; right: var(--p-pad); bottom: var(--p-pad); width: 28px; height: 28px; padding: 0; border: 1px solid var(--line); border-radius: 50%; z-index: 30;
    background: color-mix(in srgb, var(--panel) 92%, transparent); display: inline-flex; align-items: center; justify-content: center; touch-action: manipulation; cursor: pointer; }
  .fold::before { content: ''; position: absolute; inset: -8px; }
  .fold .flag { position: absolute; top: -2px; right: -2px; }
</style>
