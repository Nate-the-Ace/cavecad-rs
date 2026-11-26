<script lang="ts">
  // Settings for several picked stations, or several picked legs and splays, changed together: the passage dimensions and
  // the benchmark mark for stations; the flags that leave a shot off the map or out of the survey for shots. A value the
  // picked things share shows; where they differ the field is blank (it says VARIOUS) and a box shows as mixed. Each change
  // goes to every one of them as one undoable step in the Stations history.
  import { untrack } from 'svelte';
  import { Landmark } from '@lucide/svelte';
  import LengthField from './LengthField.svelte';
  import PanelUndo from './PanelUndo.svelte';
  import { cave, changed } from '../lib/cave.svelte';
  import { engine, type ShotDetail, type StationDetail } from '../lib/engine';

  let { kind, ids }: { kind: 'station' | 'shot'; ids: string[] } = $props();

  type Edge = 'left' | 'right' | 'up' | 'down';
  const EDGES: { key: Edge; label: string }[] = [
    { key: 'left', label: 'Left' },
    { key: 'right', label: 'Right' },
    { key: 'up', label: 'Up' },
    { key: 'down', label: 'Down' },
  ];
  let message = $state('');

  const stations = $derived.by<StationDetail[]>(() => {
    void cave.version;
    void cave.adjusted;
    return kind === 'station' ? ids.map((id) => engine.stationDetail(id, cave.adjusted)).filter((d): d is StationDetail => !!d) : [];
  });
  const shots = $derived.by<ShotDetail[]>(() => {
    void cave.version;
    return kind === 'shot' ? ids.map((id) => engine.shotDetail(id)).filter((d): d is ShotDetail => !!d) : [];
  });
  const walled = $derived(stations.filter((d) => d.walls));

  /** The value every one shares, or null when they differ (or there are none). */
  const common = <T,>(vals: T[]): T | null => (vals.length > 0 && vals.every((v) => v === vals[0]) ? vals[0] : null);

  let vals = $state<Record<Edge, number | null>>({ left: null, right: null, up: null, down: null });
  $effect(() => {
    const w = walled;
    untrack(() => {
      vals = Object.fromEntries(EDGES.map((e) => [e.key, common(w.map((d) => d.walls![e.key]))])) as Record<Edge, number | null>;
    });
  });
  // a value typed in a field goes to every station that does not already have it
  $effect(() => {
    const v = { ...vals };
    untrack(() => {
      for (const e of EDGES) {
        const now = v[e.key];
        if (now === null) continue;
        const todo = walled.filter((d) => d.walls![e.key] === null || Math.abs((d.walls![e.key] as number) - now) > 1e-9);
        if (todo.length) saveWalls(todo, { [e.key]: now });
      }
    });
  });

  function saveWalls(list: StationDetail[], patch: Record<string, number | boolean | null>) {
    let r = 'ok';
    for (const d of list) {
      const w = d.walls!;
      const next = { left: w.left, right: w.right, up: w.up, down: w.down, left_open: w.left_open, right_open: w.right_open, up_open: w.up_open, down_open: w.down_open, ...patch };
      const out = engine.setStationWalls(d.id, next);
      if (out !== 'ok') r = out;
    }
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    changed();
  }
  const openState = (e: Edge): boolean | null => common(walled.map((d) => d.walls![`${e}_open` as const]));
  const allBench = $derived(stations.length > 0 && stations.every((d) => d.benchmark));
  function benchmark() {
    let r = 'ok';
    for (const d of stations) {
      const out = engine.setStationBenchmark(d.id, !allBench);
      if (out !== 'ok') r = out;
    }
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    changed();
  }

  type Flag = 'exclude_from_plot' | 'exclude_from_all' | 'no_adjust';
  const FLAGS: { key: Flag; label: string; title: string }[] = [
    { key: 'exclude_from_plot', label: 'Not on map', title: 'Keep these shots off the plotted map' },
    { key: 'exclude_from_all', label: 'Void shot', title: 'Leave these shots out of the survey altogether' },
    { key: 'no_adjust', label: 'Hold fixed', title: 'Hold these shots as measured when the survey is adjusted' },
  ];
  const flagState = (f: Flag): boolean | null => common(shots.map((d) => d[f]));
  function setFlag(f: Flag, on: boolean) {
    let r = 'ok';
    for (const d of shots) {
      const next = { distance: d.distance, azimuth: d.azimuth, inclination: d.inclination, back_azimuth: d.back_azimuth, back_inclination: d.back_inclination, exclude_from_plot: d.exclude_from_plot, exclude_from_all: d.exclude_from_all, no_adjust: d.no_adjust, [f]: on };
      const out = engine.setShot(d.id, next);
      if (out !== 'ok') r = out;
    }
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    changed();
  }
</script>

{#if kind === 'station'}
  <div class="bulk">
    <div class="walls">
      {#each EDGES as e (e.key)}
        <span class="cell">
          <LengthField id={`bulk-lrud-${e.key}`} label={e.label} min={0} bind:value={vals[e.key]} />
          <label class="open" title="Open: looked, no wall"><input type="checkbox" checked={openState(e.key) === true} indeterminate={openState(e.key) === null && walled.length > 0} onchange={(ev) => saveWalls(walled, { [`${e.key}_open`]: ev.currentTarget.checked })} />open</label>
        </span>
      {/each}
    </div>
    <p class="hint">A blank field means the stations differ (VARIOUS); typing a value sets it for all.</p>
    <div class="btns">
      <button class="tool" class:pressed={allBench} aria-pressed={allBench} title="An agreed benchmark is drawn as a fixed survey point (a dotted triangle)" onclick={benchmark}><Landmark size={22} strokeWidth={1.6} /><span>Benchmark</span></button>
      <div class="undo"><PanelUndo scope="stations" label="station edits" /></div>
    </div>
  </div>
{:else}
  <div class="bulk">
    <div class="flags">
      {#each FLAGS as f (f.key)}
        <label title={f.title}><input type="checkbox" checked={flagState(f.key) === true} indeterminate={flagState(f.key) === null && shots.length > 0} onchange={(ev) => setFlag(f.key, ev.currentTarget.checked)} />{f.label}</label>
      {/each}
    </div>
    <div class="btns"><div class="undo"><PanelUndo scope="stations" label="survey edits" /></div></div>
  </div>
{/if}
{#if message}<div class="msg" role="alert">{message}</div>{/if}

<style>
  .bulk { display: flex; flex-direction: column; gap: 4px; width: 100%; font-size: 12px; }
  .walls { display: grid; grid-template-columns: 1fr 1fr; gap: 2px 10px; width: 100%; }
  .cell { display: flex; align-items: center; gap: 6px; white-space: nowrap; }
  .walls :global(.field) { gap: 4px; }
  .walls :global(.field label) { min-width: 34px; }
  .walls :global(input[inputmode]) { width: 4.4em; }
  .walls :global(.unit) { min-width: 1.5em; }
  .walls :global(.msg) { display: none; }
  .open, .flags label { display: inline-flex; align-items: center; gap: 6px; color: var(--muted); }
  .flags { display: flex; flex-wrap: wrap; gap: 4px 14px; }
  .hint { margin: 0; color: var(--muted); font-size: 11px; }
  .btns { display: flex; align-items: center; gap: 4px; }
  .undo { display: flex; gap: 2px; }
  .tool { display: flex; align-items: center; gap: 6px; padding: 0 10px; height: var(--p-hit); background: var(--panel-2); border: 0; border-radius: var(--p-radius); color: var(--text); font: inherit; }
  .tool.pressed { background: var(--accent); color: var(--on-accent, #fff); }
  .msg { color: var(--danger); font-size: 11px; }
</style>
