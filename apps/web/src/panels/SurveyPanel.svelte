<script lang="ts">
  // The Survey panel: the in-cave side of CaveCAD. An instrument (a Cavway X1 over Bluetooth, or the
  // simulator) sends shots; each one is written to the Survey Notebook as it arrives, and the Map, Profile
  // and Cross Section beside this panel redraw from the notebook, so the cave grows while it is being
  // surveyed. Everything here is sized for a tablet held in a glove: big targets, one thing to do next.
  // The rules (what a reading is, where the surveyor stands next) are in lib/survey.ts and tested there.
  import { untrack } from 'svelte';
  import { Check, Crosshair, SkipForward, Undo2, X } from '@lucide/svelte';
  import LengthField from '../components/LengthField.svelte';
  import PanelUndo from '../components/PanelUndo.svelte';
  import { cave, changed } from '../lib/cave.svelte';
  import { engine, loadEngine } from '../lib/engine';
  import { SIDES, SIDE_LETTER, angleText, type Side } from '../lib/notebook';
  import { formatLength } from '../lib/units';
  import * as live from '../lib/survey.svelte';
  import { survey } from '../lib/survey.svelte';
  import { nextStep } from '../lib/survey';
  import { webBluetoothAvailable } from '../lib/instrument/webble';

  let ready = $state(false);
  let showCapture = $state(false);
  loadEngine().then(() => (ready = true));

  // Set up for the open trip (not on every edit: that would move the surveyor back to the page's end).
  $effect(() => {
    void cave.trip;
    void cave.opened;
    void ready;
    untrack(() => ready && live.syncToTrip());
  });

  const tripName = $derived.by(() => {
    void cave.version;
    return ready ? (engine.notebookTrips().find((t) => t.id === cave.trip)?.name || 'Untitled trip') : '';
  });

  // The oldest thing held for Accept, and what writing it would do, for the preview card.
  const held = $derived.by(() => {
    void survey.state;
    const next = survey.held[0];
    return next ? live.preview(next) : null;
  });

  // Passage dimensions at the station the surveyor stands at.
  let wallInput = $state<Record<Side, number | null>>({ left: null, right: null, up: null, down: null });
  let wallStation = $state<string | null>(null);
  let wallsFromEngine: Record<Side, number | null> = { left: null, right: null, up: null, down: null };
  $effect(() => {
    void cave.version;
    void survey.state;
    void ready;
    untrack(() => {
      const w = ready ? live.standingWalls() : null;
      wallStation = w?.station ?? null;
      wallsFromEngine = w?.walls ?? { left: null, right: null, up: null, down: null };
      wallInput = { ...wallsFromEngine };
    });
  });
  // A dimension the person typed is written to the notebook; one the notebook refuses goes back to what it holds.
  $effect(() => {
    const typed = { ...wallInput };
    untrack(() => {
      if (wallStation === null) return;
      for (const s of SIDES) {
        if (typed[s] === wallsFromEngine[s]) continue;
        if (!live.setStandingWall(s, typed[s])) wallInput = { ...wallsFromEngine };
        break; // each write refreshes the panel, which settles the rest
      }
    });
  });

  const units = $derived(cave.units);
  const linked = $derived(survey.status === 'connected' || survey.status === 'lost');
  const noBluetooth = $derived(!webBluetoothAvailable());
  const dist = (m: number) => formatLength(m, units, 2);

  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA') return; // typing: leave the keys to the field
    if (e.key === 'Enter' && survey.held.length > 0) live.accept();
    else if (e.key === 'Escape') live.discard();
    else if (e.key === ' ' && survey.canTakeShot) {
      e.preventDefault();
      live.takeShot();
    } else if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      if (e.shiftKey ? engine.redo('notebook') : engine.undo('notebook')) changed();
    }
  }

  const enter = (e: KeyboardEvent) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur();
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions: the panel takes Space, Enter, Escape and Ctrl/Cmd+Z for the survey -->
<div class="panel" role="region" aria-label="Survey" tabindex="-1" onkeydown={onKey}>
  <div class="bar">
    <PanelUndo scope="notebook" label="in the notebook" />
    <span class="chip" class:on={survey.status === 'connected'} class:warn={survey.status === 'lost' || survey.status === 'error'} role="status">
      {#if survey.status === 'connected'}{survey.device || 'Connected'}
      {:else if survey.status === 'connecting'}Connecting…
      {:else if survey.status === 'lost'}Reconnecting…
      {:else if survey.status === 'error'}Could not connect
      {:else if survey.status === 'unsupported'}No Bluetooth here
      {:else}No instrument{/if}
    </span>
    <span class="trip" title="The notebook page shots are written to (choose it in the Notebook)">{tripName}</span>
  </div>

  <div class="scroll">
    {#if survey.detail && survey.status !== 'connected'}
      <p class="banner" role="alert">{survey.detail}</p>
    {:else if noBluetooth && !linked}
      <p class="banner">This browser has no Web Bluetooth. On an iPad, open CaveCAD in the <b>Bluefy</b> browser to connect an instrument. The simulator works anywhere.</p>
    {/if}

    <section class="reading" aria-label="Latest reading">
      {#if survey.last}
        <div class="big"><b>{dist(survey.last.distance)}</b><small>tape</small></div>
        <div class="big"><b>{angleText(+survey.last.azimuth.toFixed(1))}°</b><small>compass</small></div>
        <div class="big"><b>{angleText(+survey.last.inclination.toFixed(1))}°</b><small>clino</small></div>
      {:else}
        <p class="idle">{linked ? 'Waiting for a shot…' : 'Connect an instrument in the Survey tab, or try the simulator.'}</p>
      {/if}
    </section>

    <div class="notes">
      {#if survey.message}<p class="problem" role="alert">{survey.message}</p>{/if}
      {#each survey.warnings as w}<p class="caution" role="status">{w}</p>{/each}
    </div>

    {#if held}
      <section class="hold" aria-label="Reading waiting">
        {#if !held.reject}
          <p>Write this as <b>{held.text}</b>?{#if survey.held.length > 1} <span class="hint">({survey.held.length - 1} more waiting)</span>{/if}</p>
          {#each held.warnings as w}<p class="caution">{w}</p>{/each}
        {:else}<p class="problem">{held.reject}</p>{/if}
        <div class="row">
          <button class="go" onclick={live.accept} disabled={held.reject !== null}><Check size={20} /> Accept</button>
          <button class="no" onclick={live.discard}><X size={20} /> Discard</button>
        </div>
      </section>
    {/if}

    <section class="where" aria-label="Where you are">
      <div class="names">
        <label>From <input class="name" bind:value={survey.state.from} maxlength="40" aria-label="Station you are at" onkeydown={enter} disabled={survey.state.awaiting !== null} /></label>
        <span aria-hidden="true">→</span>
        <label>To <input class="name" bind:value={survey.state.to} maxlength="40" aria-label="Station the next shot goes to" onkeydown={enter} disabled={survey.state.mode === 'splay' || survey.state.awaiting !== null} /></label>
      </div>
      <p class="next" role="status">{nextStep(survey.state)}</p>
      {#if survey.groupsLegs && survey.waiting > 0}
        <p class="wait" role="status">{survey.waiting} close shot{survey.waiting === 1 ? '' : 's'} so far: {survey.waiting === 1 ? 'two more make' : 'one more makes'} a leg. A pause of a few seconds makes {survey.waiting === 1 ? 'it a splay' : 'them splays'}. <button class="link" onclick={live.settleNow}>Splays now</button></p>
      {:else}
        <p class="wait" aria-hidden="true">&nbsp;</p>
      {/if}
    </section>

    <section class="walls" aria-label="Passage dimensions">
      <h4>Passage at {wallStation ?? 'this station'}</h4>
      {#if wallStation}
        <div class="lrud">
          {#each SIDES as s}
            <span class="lr"><b>{SIDE_LETTER[s]}</b><LengthField id="survey-{s}" bind:value={wallInput[s]} min={0} /></span>
          {/each}
        </div>
      {:else}
        <p class="idle">Appears once this station has a line in the notebook (take its shot first).</p>
      {/if}
    </section>

    <section class="log" aria-label="Shots this session">
      <h4>This session</h4>
      {#each [...survey.log].reverse() as e (e.n)}
        <div class="entry" class:flag={e.warnings.length > 0} title={e.warnings.join(' ')}>
          <b>{e.label}</b>
          <span>{dist(e.reading.distance)} · {angleText(+e.reading.azimuth.toFixed(1))}° · {angleText(+e.reading.inclination.toFixed(1))}°</span>
        </div>
      {:else}
        <p class="idle">Nothing yet.</p>
      {/each}
    </section>

    <details class="capture" bind:open={showCapture}>
      <summary>Instrument log, and capture for an instrument that is not supported yet</summary>
      <p class="hint">Capture connects to any Bluetooth device and logs every frame it sends. The browser can only reach services it is told about in advance: list the service UUIDs (from a Bluetooth scanner app such as nRF Connect), separated by spaces or commas.</p>
      <label class="chk"><input type="checkbox" bind:checked={survey.anyDevice} /> Show every Bluetooth device when connecting (if the instrument is not offered by default)</label>
      <textarea bind:value={survey.services} rows="2" placeholder="service uuids, e.g. 0000ffe0-0000-1000-8000-00805f9b34fb" aria-label="Service UUIDs to reach"></textarea>
      <div class="row">
        <button class="small" onclick={() => live.connect('capture')} disabled={linked}>Connect in capture mode</button>
        <button class="small" onclick={() => navigator.clipboard?.writeText(survey.frames.join('\n'))} disabled={survey.frames.length === 0}>Copy log</button>
        <button class="small" onclick={() => (survey.frames = [])} disabled={survey.frames.length === 0}>Clear</button>
      </div>
      <pre aria-label="Frames received">{survey.frames.join('\n') || 'No frames yet.'}</pre>
    </details>
  </div>

  <footer class="dock" aria-label="Actions">
    <button class="shoot" onclick={live.takeShot} disabled={!survey.canTakeShot}><Crosshair size={22} /> {survey.kind === 'simulator' ? 'Shoot station' : 'Take shot'}</button>
    <div class="slots">
      <button onclick={live.undoLast} disabled={survey.log.length === 0}><Undo2 size={18} /> Undo last shot</button>
      {#if survey.state.awaiting}
        <button onclick={live.skipBacksight}><SkipForward size={18} /> No backsight</button>
      {:else}
        <button onclick={live.nextStation} disabled={survey.state.mode === 'splay'} title="Move on to the next station without a shot">Next station</button>
      {/if}
    </div>
  </footer>
</div>

<style>
  .panel { display: flex; flex-direction: column; height: 100%; background: var(--panel); color: var(--text); font-size: 14px; outline: none; }
  .bar { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 6px 10px; border-bottom: 1px solid var(--line); }
  .scroll { flex: 1; overflow-y: auto; overflow-x: hidden; padding: 10px; display: flex; flex-direction: column; gap: 12px; }
  .chip { padding: 3px 10px; border-radius: 999px; border: 1px solid var(--line); color: var(--muted); font-size: 12px; }
  .chip.on { border-color: var(--accent); color: var(--text); }
  .chip.warn { border-color: var(--warn); color: var(--warn); }
  .trip { color: var(--muted); font-size: 12px; max-width: 14em; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  button { font: inherit; padding: 10px 14px; min-height: 44px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); cursor: pointer; display: inline-flex; align-items: center; justify-content: center; gap: 6px; touch-action: manipulation; }
  button:disabled { opacity: 0.45; cursor: default; }
  button.small { min-height: 34px; padding: 4px 10px; font-size: 13px; }
  button.go, button.shoot { background: var(--accent); border-color: var(--accent); color: var(--on-accent, #fff); }
  button.no { color: var(--danger); }
  button.shoot { font-size: 17px; font-weight: 600; min-height: 56px; width: 100%; }
  .banner { margin: 0; padding: 8px 10px; border: 1px solid var(--warn); border-radius: var(--radius); font-size: 13px; }
  .reading { display: flex; flex-wrap: wrap; gap: 8px; min-height: 88px; align-items: stretch; }
  .notes { min-height: 3.4em; }
  .dock { border-top: 1px solid var(--line); padding: 8px 10px; display: flex; flex-direction: column; gap: 8px; background: var(--panel); }
  .slots { display: grid; grid-template-columns: repeat(2, 1fr); gap: 8px; }
  .slots button { padding: 8px 6px; font-size: 13px; }
  .big { flex: 1 1 70px; min-width: 0; text-align: center; padding: 10px 4px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); }
  .big b { display: block; font-size: 26px; font-variant-numeric: tabular-nums; }
  .big small { color: var(--muted); font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; }
  .idle { color: var(--muted); margin: 6px 0; align-self: center; }
  .problem { margin: 0; color: var(--danger); }
  .caution { margin: 0; color: var(--warn); font-size: 13px; }
  .hold { padding: 10px; border: 1px solid var(--accent); border-radius: var(--radius); }
  .hold p { margin: 0 0 8px; }
  .row { display: flex; flex-wrap: wrap; gap: 8px; }
  .row button { flex: 1; }
  .where { display: flex; flex-wrap: wrap; align-items: center; gap: 10px 14px; }
  .chk { display: inline-flex; align-items: center; gap: 6px; min-height: 44px; }
  .chk input { width: 20px; height: 20px; }
  .names { display: flex; align-items: center; gap: 8px; flex: 1 1 100%; }
  .names label { display: inline-flex; align-items: center; gap: 6px; color: var(--muted); }
  input.name { width: 6.5em; font: inherit; font-size: 18px; font-weight: 600; padding: 6px 8px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); }
  input.name:disabled { opacity: 0.5; }
  button.link { min-height: 0; padding: 2px 8px; font-size: 12px; margin-left: 6px; }
  .wait { margin: 0; flex: 1 1 100%; font-size: 13px; color: var(--accent); min-height: 2.6em; }
  .next { margin: 0; flex: 1 1 100%; font-size: 15px; font-weight: 600; min-height: 2.6em; }
  h4 { margin: 0 0 6px; font-size: 12px; color: var(--muted); text-transform: uppercase; letter-spacing: 0.06em; font-weight: 600; }
  .lrud { display: flex; flex-wrap: wrap; gap: 10px; }
  .lr { display: inline-flex; align-items: center; gap: 6px; }
  .lr :global(input) { font-size: 16px; padding: 8px; width: 5.5em; }
  .hint { color: var(--muted); font-size: 12px; }
  .log .entry { display: flex; justify-content: space-between; gap: 10px; padding: 4px 0; border-bottom: 1px solid var(--line); font-size: 13px; }
  .log .entry span { color: var(--muted); font-variant-numeric: tabular-nums; }
  .log .entry.flag b { color: var(--warn); }
  .capture summary { cursor: pointer; color: var(--muted); font-size: 13px; padding: 6px 0; }
  .capture textarea { width: 100%; box-sizing: border-box; font: inherit; font-size: 12px; padding: 6px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); margin: 6px 0; }
  .capture pre { max-height: 200px; overflow: auto; font-size: 11px; background: var(--panel-2); padding: 6px; border-radius: var(--radius); margin: 8px 0 0; white-space: pre-wrap; word-break: break-all; }
</style>
