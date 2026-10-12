<script lang="ts">
  // The groups of the station tab: what the survey says about the picked station (kind of place, legs,
  // elevation), its passage dimensions and notes (editable: each edit goes to the shot that holds it,
  // as one undoable step in the Stations history), and whether it is an agreed benchmark. Elevation
  // only; a station's plan position is never shown.
  import { untrack } from 'svelte';
  import { Check, Landmark, Scissors } from '@lucide/svelte';
  import LengthField from './LengthField.svelte';
  import PanelUndo from './PanelUndo.svelte';
  import { cave, changed } from '../lib/cave.svelte';
  import { engine, loadEngine, type StationDetail } from '../lib/engine';
  import { showPanel } from '../lib/workspace.svelte';
  import { formatLength } from '../lib/units';

  /** `measure` marks the ribbon's unseen copy used to measure widths: it never writes. */
  let { part, measure = false }: { part: 'info' | 'walls' | 'notes' | 'mark'; measure?: boolean } = $props();

  let detail = $state<StationDetail | null>(null);
  let ready = $state(false);
  let message = $state('');
  loadEngine().then(() => (ready = true));

  type Edge = 'left' | 'right' | 'up' | 'down';
  const EDGES: { key: Edge; label: string }[] = [
    { key: 'left', label: 'Left' },
    { key: 'right', label: 'Right' },
    { key: 'up', label: 'Up' },
    { key: 'down', label: 'Down' },
  ];
  // what the fields hold while typing
  let vals = $state<Record<Edge, number | null>>({ left: null, right: null, up: null, down: null });
  let note = $state(0);
  let words = $state('');

  $effect(() => {
    void cave.version;
    void cave.adjusted;
    void cave.picked;
    void ready;
    untrack(() => {
      detail = ready && cave.picked ? engine.stationDetail(cave.picked, cave.adjusted) : null;
      if (detail?.walls) vals = { left: detail.walls.left, right: detail.walls.right, up: detail.walls.up, down: detail.walls.down };
      if (note >= (detail?.notes.length ?? 0)) note = 0;
      words = detail?.notes[note]?.text ?? '';
    });
  });
  $effect(() => {
    void note;
    untrack(() => (words = detail?.notes[note]?.text ?? ''));
  });

  function saveWalls(patch: Partial<Record<Edge, number | null>> = {}, open: Partial<Record<`${Edge}_open`, boolean>> = {}) {
    if (measure || !detail?.walls) return;
    const w = detail.walls;
    const next = {
      left: w.left, right: w.right, up: w.up, down: w.down,
      left_open: w.left_open, right_open: w.right_open, up_open: w.up_open, down_open: w.down_open,
      ...patch,
      ...open,
    };
    const r = engine.setStationWalls(detail.id, next);
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    changed();
  }
  // a value committed in a field goes to the survey
  $effect(() => {
    const v = { ...vals };
    untrack(() => {
      const w = detail?.walls;
      if (!w) return;
      const changedEdges = EDGES.filter((e) => (v[e.key] ?? null) !== (w[e.key] ?? null) && !(v[e.key] !== null && w[e.key] !== null && Math.abs((v[e.key] as number) - (w[e.key] as number)) < 1e-9));
      if (changedEdges.length) saveWalls(Object.fromEntries(changedEdges.map((e) => [e.key, v[e.key]])));
    });
  });

  function saveNote() {
    const n = detail?.notes[note];
    if (!n || words.trim() === n.text) return;
    const r = engine.setStationNote(n.source, words);
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    changed();
  }

  function benchmark() {
    if (!detail) return;
    const r = engine.setStationBenchmark(detail.id, !detail.benchmark);
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    changed();
  }

  const KIND: Record<string, string> = { lead: 'Lead', openEnd: 'Open end', junction: 'Junction', control: 'Control', loop: 'Loop', noted: 'Noted', flagged: 'Flagged' };
</script>

{#if !detail}
  <div class="none">Pick a station in the Map, Profile or Stations table.</div>
{:else if part === 'info'}
  <div class="col info">
    <div class="row"><strong class="label">{detail.label}</strong>{#if detail.benchmark}<span class="tag">Benchmark</span>{/if}<span class="muted">{detail.z === null ? '' : formatLength(detail.z, cave.units) + ' elevation'}</span></div>
    <div class="row muted" title={detail.flags.join('; ')}>{detail.degree} {detail.degree === 1 ? 'leg' : 'legs'} · {detail.kinds.map((k) => KIND[k] ?? k).join(', ') || 'Plain station'}</div>
  </div>
{:else if part === 'walls'}
  {#if detail.walls}
    <div class="walls" title={`Kept on the ${detail.walls.on}`}>
      {#each EDGES as e}
        {@const open = detail.walls[`${e.key}_open` as const]}
        <span class="cell">
          <LengthField id={`lrud-${e.key}`} label={e.label} min={0} bind:value={vals[e.key]} />
          <label class="open" title="Open: looked, no wall"><input type="checkbox" checked={open} onchange={(ev) => saveWalls({}, { [`${e.key}_open`]: ev.currentTarget.checked })} />open</label>
        </span>
      {/each}
    </div>
  {:else}
    <div class="none">No shot reaches this station.</div>
  {/if}
{:else if part === 'notes'}
  {#if detail.notes.length}
    <div class="col notes">
      <select bind:value={note} aria-label="Which note" title="The notes written at this station, one for each shot that arrives">
        {#each detail.notes as n, i}<option value={i}>{n.on}{n.text ? '' : ' (empty)'}</option>{/each}
      </select>
      <input type="text" bind:value={words} maxlength="200" aria-label="Note" placeholder="Note" onchange={saveNote} onkeydown={(e) => e.key === 'Enter' && saveNote()} />
    </div>
  {:else}
    <div class="none">No shot reaches this station.</div>
  {/if}
{:else}
  <div class="btns">
    <button class="tool" class:pressed={detail.benchmark} aria-pressed={detail.benchmark} title="An agreed benchmark is drawn as a fixed survey point (a dotted triangle)" onclick={benchmark}><Landmark size={22} strokeWidth={1.6} /><span>Benchmark</span></button>
    <button class="tool" title="Show this station's cross section" onclick={() => (showPanel('section'), (cave.station = detail!.id))}><Scissors size={22} strokeWidth={1.6} /><span>Section</span></button>
    <div class="undo"><PanelUndo scope="stations" label="station edits" /></div>
    <button class="tool" onclick={() => (cave.picked = '')}><Check size={22} strokeWidth={1.6} /><span>Done</span></button>
  </div>
{/if}
{#if message && part === 'mark'}<div class="msg" role="alert">{message}</div>{/if}

<style>
  .none { color: var(--muted); font-size: 12px; align-self: center; max-width: 220px; }
  .col { display: flex; flex-direction: column; gap: 4px; justify-content: center; height: 100%; font-size: 12px; min-width: 210px; overflow: hidden; }
  .row { display: flex; align-items: center; gap: 8px; height: 22px; white-space: nowrap; }
  .label { font-size: 15px; color: var(--text-strong); }
  .muted { color: var(--muted); }
  .tag { padding: 0 6px; border-radius: 9px; background: var(--accent-bg); color: var(--accent); }
  .walls { display: grid; grid-template-columns: auto auto; grid-template-rows: 26px 26px; gap: 2px 12px; align-content: center; height: 100%; }
  .cell { display: flex; align-items: center; gap: 6px; white-space: nowrap; }
  .walls :global(.field) { gap: 4px; }
  .walls :global(.field label) { min-width: 34px; }
  .walls :global(input[inputmode]) { width: 4.4em; }
  .walls :global(.unit) { min-width: 1.5em; }
  .walls :global(.msg) { display: none; }
  .open { display: inline-flex; align-items: center; gap: 3px; font-size: 11px; color: var(--muted); }
  select, input[type='text'] { height: 22px; box-sizing: border-box; padding: 0 6px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); font: inherit; min-width: 0; }
  .notes { min-width: 260px; }
  .btns { display: flex; align-items: center; gap: 4px; height: 100%; }
  .undo { display: flex; flex-direction: column; gap: 2px; }
  .undo :global(button) { padding: 2px 6px; }
  .tool { display: flex; flex-direction: column; align-items: center; gap: 4px; min-width: 58px; box-sizing: border-box; height: calc(var(--ribbon-body) - 1px); justify-content: center; overflow: hidden; padding: 4px 8px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; color: var(--text); }
  .tool:hover { background: var(--panel-2); }
  .tool.pressed { background: var(--accent-bg); outline: 1px solid var(--accent); }
  .msg { position: absolute; font-size: 11px; color: var(--danger); }
</style>
