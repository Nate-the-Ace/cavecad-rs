<script lang="ts">
  // The groups of the tab for a picked survey leg or splay: where it runs, what was measured (tape,
  // bearing, inclination, and the back readings), its note, and the flags that leave it off the map or out
  // of the survey. Edits go to the shot as one undoable step in the Stations history.
  import { untrack } from 'svelte';
  import { Check } from '@lucide/svelte';
  import LengthField from './LengthField.svelte';
  import PanelUndo from './PanelUndo.svelte';
  import { cave, changed, clearPick } from '../lib/cave.svelte';
  import { engine, loadEngine, type ShotDetail, type ShotEdit } from '../lib/engine';

  /** `measure` marks the ribbon's unseen copy used to measure widths: it never writes. */
  let { part, measure = false }: { part: 'info' | 'reading' | 'note' | 'mark'; measure?: boolean; kind?: string } = $props();

  let detail = $state<ShotDetail | null>(null);
  let ready = $state(false);
  let message = $state('');
  loadEngine().then(() => (ready = true));

  let distance = $state<number | null>(null);
  let angles = $state({ azimuth: '', inclination: '', back_azimuth: '', back_inclination: '' });
  let words = $state('');

  const text = (v: number | null) => (v === null ? '' : String(Math.round(v * 100) / 100));
  $effect(() => {
    void cave.version;
    void cave.pick;
    void ready;
    untrack(() => {
      detail = ready && (cave.pick.kind === 'leg' || cave.pick.kind === 'splay') ? engine.shotDetail(cave.pick.id) : null;
      if (!detail) return;
      distance = detail.distance;
      angles = { azimuth: text(detail.azimuth), inclination: text(detail.inclination), back_azimuth: text(detail.back_azimuth), back_inclination: text(detail.back_inclination) };
      words = detail.note;
    });
  });

  function save(patch: Partial<ShotEdit>) {
    if (measure || !detail) return;
    const d = detail;
    const next: ShotEdit = { distance: d.distance, azimuth: d.azimuth, inclination: d.inclination, back_azimuth: d.back_azimuth, back_inclination: d.back_inclination, exclude_from_plot: d.exclude_from_plot, exclude_from_all: d.exclude_from_all, no_adjust: d.no_adjust, ...patch };
    const r = engine.setShot(d.id, next);
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    changed();
  }
  // a distance committed in the field goes to the survey
  $effect(() => {
    const v = distance;
    untrack(() => {
      if (detail && (v ?? null) !== (detail.distance ?? null) && !(v !== null && detail.distance !== null && Math.abs(v - detail.distance) < 1e-9)) save({ distance: v });
    });
  });

  function angle(key: 'azimuth' | 'inclination' | 'back_azimuth' | 'back_inclination') {
    const t = angles[key].trim();
    if (t === '') return save({ [key]: null });
    const n = Number(t.replace('°', ''));
    if (!Number.isFinite(n)) {
      message = 'Not a number of degrees';
      angles[key] = text(detail?.[key] ?? null);
      return;
    }
    save({ [key]: n });
  }

  function saveNote() {
    if (measure || !detail || words.trim() === detail.note) return;
    const r = engine.setStationNote(detail.id, words);
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    changed();
  }
</script>

{#if !detail}
  <div class="none">Pick a leg or splay on the Map.</div>
{:else if part === 'info'}
  <div class="col">
    <div class="row"><strong class="label">{detail.splay ? `${detail.from} splay` : `${detail.from} → ${detail.to}`}</strong></div>
    <div class="row muted" title={detail.problem}>{detail.trip_name || `Trip ${detail.trip + 1}`}{detail.problem ? ` · ${detail.problem}` : ''}</div>
  </div>
{:else if part === 'reading'}
  <div class="grid">
    <LengthField id="shot-distance" label="Tape" min={0} bind:value={distance} />
    <label class="a">Azimuth<input bind:value={angles.azimuth} inputmode="decimal" aria-label="Azimuth" onchange={() => angle('azimuth')} />°</label>
    <label class="a">Incl.<input bind:value={angles.inclination} inputmode="decimal" aria-label="Inclination" onchange={() => angle('inclination')} />°</label>
    <span></span>
    <label class="a">Back az.<input bind:value={angles.back_azimuth} inputmode="decimal" aria-label="Back azimuth" onchange={() => angle('back_azimuth')} />°</label>
    <label class="a">Back incl.<input bind:value={angles.back_inclination} inputmode="decimal" aria-label="Back inclination" onchange={() => angle('back_inclination')} />°</label>
  </div>
{:else if part === 'note'}
  <div class="col note">
    <input type="text" bind:value={words} maxlength="200" aria-label="Shot note" placeholder="Note" onchange={saveNote} onkeydown={(e) => e.key === 'Enter' && saveNote()} />
    <div class="row">
      <label title="Keep this shot off the plotted map"><input type="checkbox" checked={detail.exclude_from_plot} onchange={(e) => save({ exclude_from_plot: e.currentTarget.checked })} />Not on map</label>
      <label title="Leave this shot out of the survey altogether (a void shot)"><input type="checkbox" checked={detail.exclude_from_all} onchange={(e) => save({ exclude_from_all: e.currentTarget.checked })} />Void</label>
      <label title="Hold this shot as measured when the survey is adjusted"><input type="checkbox" checked={detail.no_adjust} onchange={(e) => save({ no_adjust: e.currentTarget.checked })} />Hold fixed</label>
    </div>
  </div>
{:else}
  <div class="btns">
    <div class="undo"><PanelUndo scope="stations" label="survey edits" /></div>
    <button class="tool" onclick={clearPick}><Check size={22} strokeWidth={1.6} /><span>Done</span></button>
  </div>
  {#if message}<div class="msg" role="alert">{message}</div>{/if}
{/if}

<style>
  .none { color: var(--muted); font-size: 12px; align-self: center; max-width: 220px; }
  .col { display: flex; flex-direction: column; gap: 4px; justify-content: center; height: 100%; font-size: 12px; min-width: 190px; overflow: hidden; }
  .row { display: flex; align-items: center; gap: 10px; height: 22px; white-space: nowrap; }
  .label { font-size: 15px; color: var(--text-strong); }
  .muted { color: var(--muted); }
  .grid { display: grid; grid-template-columns: auto auto auto; grid-template-rows: 26px 26px; gap: 2px 12px; align-content: center; height: 100%; font-size: 12px; }
  .grid :global(.field) { gap: 4px; }
  .grid :global(.field label) { min-width: 48px; }
  .grid :global(input[inputmode]) { width: 4.6em; }
  .grid :global(.msg) { display: none; }
  .a { display: inline-flex; align-items: center; gap: 4px; white-space: nowrap; }
  .a input { width: 4.4em; font: inherit; padding: 3px 6px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); color: var(--text); text-align: right; }
  .a :global(:first-child) { min-width: 0; }
  .note { min-width: 300px; }
  .note label { display: inline-flex; align-items: center; gap: 3px; font-size: 11px; color: var(--muted); }
  input[type='text'] { height: 22px; box-sizing: border-box; padding: 0 6px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); font: inherit; min-width: 0; }
  .btns { display: flex; align-items: center; gap: 4px; height: 100%; }
  .undo { display: flex; flex-direction: column; gap: 2px; }
  .undo :global(button) { padding: 2px 6px; }
  .tool { display: flex; flex-direction: column; align-items: center; gap: 4px; min-width: 58px; box-sizing: border-box; height: calc(var(--ribbon-body) - 1px); justify-content: center; overflow: hidden; padding: 4px 8px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; color: var(--text); }
  .tool:hover { background: var(--panel-2); }
  .msg { position: absolute; font-size: 11px; color: var(--danger); }
</style>
