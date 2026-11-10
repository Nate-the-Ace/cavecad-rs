<script lang="ts">
  // Survey timeline: plays the cave growing trip by trip, in survey order. It only shows and hides
  // trips (the same switch as the Show menu's Trips list), so nothing in the cave changes; when it is
  // closed the trips shown before it started come back exactly as they were.
  import { onDestroy } from 'svelte';
  import { Play, Pause, X, SkipBack } from '@lucide/svelte';
  import { filter } from '../lib/cave.svelte';
  import { engine } from '../lib/engine';

  let { trips, onChange, onClose }: { trips: { id: string; name: string }[]; onChange: () => void; onClose: () => void } = $props();

  /** The trips shown before the timeline took over, given back on close. */
  const before = [...filter.hiddenTrips];
  /** Dates from the notebook, by trip id (the scene lists names only). */
  const dates = new Map<string, string>();
  try {
    for (const t of engine.notebookTrips()) dates.set(t.id, t.date);
  } catch {
    // no notebook for this cave: the names are enough
  }

  let step = $state(1); // how many trips are showing
  let playing = $state(false);
  /** Seconds each trip stays before the next one appears. */
  let pace = $state(1.2);
  let timer: ReturnType<typeof setInterval> | undefined;

  function show(n: number) {
    step = Math.max(1, Math.min(trips.length, n));
    filter.hiddenTrips = trips.slice(step).map((t) => t.id);
    onChange();
  }
  function stopTimer() {
    if (timer) clearInterval(timer);
    timer = undefined;
    playing = false;
  }
  function play() {
    if (playing) return stopTimer();
    if (step >= trips.length) show(1); // at the end: start over
    playing = true;
    timer = setInterval(() => {
      if (step >= trips.length) return stopTimer();
      show(step + 1);
    }, pace * 1000);
  }
  function close() {
    stopTimer();
    filter.hiddenTrips = before;
    onChange();
    onClose();
  }
  // starts at the first trip, playing
  show(1);
  play();
  onDestroy(stopTimer);

  const current = $derived(trips[step - 1]);
  const date = $derived(current ? (dates.get(current.id) ?? '') : '');
</script>

<div class="timeline" role="group" aria-label="Survey timeline">
  <button class="icon" title="Back to the first trip" onclick={() => (stopTimer(), show(1))}><SkipBack size={15} /></button>
  <button class="icon" title={playing ? 'Pause' : 'Play: each trip appears in turn'} onclick={play}>
    {#if playing}<Pause size={15} />{:else}<Play size={15} />{/if}
  </button>
  <input type="range" min="1" max={trips.length} step="1" value={step} aria-label="Trip" oninput={(e) => (stopTimer(), show(Number(e.currentTarget.value)))} />
  <div class="caption">
    <strong>Trip {step} of {trips.length}</strong>
    <span>{current?.name || 'Unnamed trip'}{date ? ` · ${date}` : ''}</span>
  </div>
  <label class="pace" title="Seconds each trip shows before the next appears">
    <select bind:value={pace} onchange={() => playing && (stopTimer(), play())}>
      <option value={0.5}>Fast</option>
      <option value={1.2}>Normal</option>
      <option value={2.5}>Slow</option>
    </select>
  </label>
  <button class="icon" title="Close the timeline and show the trips as they were" onclick={close}><X size={15} /></button>
</div>

<style>
  .timeline { position: absolute; left: 50%; bottom: 14px; transform: translateX(-50%); display: flex; align-items: center; gap: 8px; padding: 8px 10px; max-width: calc(100% - 32px); box-sizing: border-box; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); font-size: 12px; box-shadow: 0 4px 18px rgba(0, 0, 0, 0.25); z-index: 5; }
  .icon { display: inline-flex; align-items: center; justify-content: center; width: 28px; height: 28px; padding: 0; background: none; border: 1px solid var(--line); border-radius: var(--radius); color: var(--text); cursor: pointer; }
  .icon:hover { background: var(--panel-2); }
  input[type='range'] { width: 160px; min-width: 60px; flex: 1 1 auto; }
  .caption { display: flex; flex-direction: column; min-width: 0; max-width: 260px; }
  .caption span { color: var(--muted); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  select { font-size: 12px; }
</style>
