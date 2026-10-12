<script lang="ts">
  // The station table: one row per station, with what kind of place it is (lead, open end, junction,
  // control, loop, noted, flagged), the notes written at it, and the team's own status, note and who
  // has it, typed straight into the row. The rows are worked out from the survey every time; only the
  // team's mark is stored (so everyone editing the cave shares it). Double-click a row to go there.
  import { untrack } from 'svelte';
  import { Download } from '@lucide/svelte';
  import PanelUndo from '../components/PanelUndo.svelte';
  import { cave, changed, focusStation, setPick } from '../lib/cave.svelte';
  import { engine, loadEngine } from '../lib/engine';
  import { showPanel } from '../lib/workspace.svelte';
  import { KINDS, STATUSES, checklistCsv, effectiveStatus, filterRows, kindCounts, type StationRow } from '../lib/station-table';
  import { formatLength } from '../lib/units';

  let rows = $state<StationRow[]>([]);
  let ready = $state(false);
  let kinds = $state<string[]>([]);
  let text = $state('');
  let status = $state('');
  let message = $state('');
  loadEngine().then(() => (ready = true));

  $effect(() => {
    void cave.version;
    void cave.adjusted;
    void ready;
    untrack(() => {
      if (ready) rows = engine.stationRows(cave.adjusted);
    });
  });

  const shown = $derived(filterRows(rows, { kinds, text, status }));
  const counts = $derived(kindCounts(rows));

  const toggle = (k: string) => (kinds = kinds.includes(k) ? kinds.filter((x) => x !== k) : [...kinds, k]);

  /** Saves one row's mark (only what changed is written). */
  function mark(row: StationRow, patch: Partial<Pick<StationRow, 'status' | 'team' | 'who'>>) {
    const next = { status: row.status, team: row.team, who: row.who, ...patch };
    const r = engine.setStationMark(row.id, next.status, next.team, next.who);
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    changed();
  }

  function pick(row: StationRow) {
    if (!row.id) return;
    setPick('station', row.id);
    cave.station = row.id;
  }

  function go(row: StationRow) {
    if (!row.id) return;
    cave.station = row.id;
    focusStation(row.id);
    showPanel('section');
  }

  function download() {
    const csv = checklistCsv(shown, cave.units);
    const url = URL.createObjectURL(new Blob([csv], { type: 'text/csv' }));
    const a = document.createElement('a');
    a.href = url;
    a.download = cave.fileName.replace(/\.[^.]+$/, '') + '-stations.csv';
    a.click();
    URL.revokeObjectURL(url);
  }

  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (t.tagName === 'INPUT' || t.tagName === 'SELECT') return; // typing: leave the keys to the field
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      if (e.shiftKey ? engine.redo('stations') : engine.undo('stations')) changed();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions: the panel takes Ctrl/Cmd+Z for its own undo -->
<div class="panel" role="region" aria-label="Station table" tabindex="-1" onkeydown={onKey}>
  <div class="bar">
    <PanelUndo scope="stations" label="in the station table" />
    <div class="chips" role="group" aria-label="Kinds of station">
      {#each KINDS as k}
        <button class="chip" class:on={kinds.includes(k.key)} aria-pressed={kinds.includes(k.key)} title={k.hint} onclick={() => toggle(k.key)}>
          {k.label} <span class="n">{counts[k.key] ?? 0}</span>
        </button>
      {/each}
    </div>
    <input type="search" placeholder="Search stations and notes" bind:value={text} aria-label="Search stations and notes" />
    <select bind:value={status} aria-label="Status">
      <option value="">Any status</option>
      {#each STATUSES as s}<option value={s}>{s}</option>{/each}
    </select>
    <span class="count">{shown.length} of {rows.length}</span>
    <button class="icon" title="Download the stations shown as a checklist (CSV): elevations only, never positions" aria-label="Download checklist" onclick={download} disabled={shown.length === 0}><Download size={15} /></button>
    {#if message}<span class="msg" role="status">{message}</span>{/if}
  </div>
  <div class="scroll">
    <table>
      <thead>
        <tr><th>Station</th><th>Kinds</th><th>Trips</th><th>Elevation</th><th>Status</th><th>Note in survey</th><th>Team notes</th><th>Assigned</th></tr>
      </thead>
      <tbody>
        {#each shown as r (r.id || r.station)}
          <tr onclick={() => pick(r)} ondblclick={() => go(r)} class:sel={cave.pick.kind === 'station' ? cave.pick.id === r.id : cave.station === r.id} title="Double-click to go to this station">
            <td class="st">{r.station}</td>
            <td class="kinds">
              {#each r.kinds as k}<span class="badge {k}">{KINDS.find((x) => x.key === k)?.label ?? k}</span>{/each}
            </td>
            <td class="num">{r.trips.map((t) => t + 1).join(', ')}</td>
            <td class="num">{r.z === null ? '' : formatLength(r.z, cave.units, 1)}</td>
            <td>
              <select value={r.status} aria-label="Status of {r.station}" onchange={(e) => mark(r, { status: e.currentTarget.value })}>
                <option value="">{effectiveStatus(r) === 'open' && !r.status ? 'open (a lead)' : ''}</option>
                {#each STATUSES as s}<option value={s}>{s}</option>{/each}
              </select>
              {#if r.suggest && !r.status}<button class="hint" title="A later trip surveyed on from this lead. Mark it pushed?" onclick={() => mark(r, { status: 'pushed' })}>pushed?</button>{/if}
            </td>
            <td class="note" title={r.note_text}>{r.note_text}</td>
            <td><input value={r.team} aria-label="Team note for {r.station}" maxlength="200" onchange={(e) => mark(r, { team: e.currentTarget.value })} /></td>
            <td><input value={r.who} aria-label="Who has {r.station}" maxlength="200" onchange={(e) => mark(r, { who: e.currentTarget.value })} /></td>
          </tr>
        {:else}
          <tr><td colspan="8" class="none">{rows.length ? 'No station matches.' : 'Open a cave to see its stations.'}</td></tr>
        {/each}
      </tbody>
    </table>
  </div>
</div>

<style>
  .panel { display: flex; flex-direction: column; height: 100%; background: var(--panel); color: var(--text); font-size: 13px; outline: none; }
  .bar { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 6px 10px; border-bottom: 1px solid var(--line); }
  .chips { display: flex; flex-wrap: wrap; gap: 4px; }
  .chip { padding: 2px 8px; border-radius: 999px; border: 1px solid var(--line); background: none; color: var(--text); cursor: pointer; font-size: 12px; }
  .chip.on { background: var(--accent-bg); border-color: var(--accent); }
  .chip .n { color: var(--muted); font-size: 11px; }
  input, select { font: inherit; font-size: 12px; padding: 3px 6px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); }
  input[type='search'] { width: 190px; }
  .count { color: var(--muted); font-size: 12px; }
  .icon { display: inline-grid; place-items: center; padding: 4px 6px; border: 1px solid var(--line); border-radius: var(--radius); background: none; color: var(--text); cursor: pointer; }
  .icon:disabled { opacity: 0.4; cursor: default; }
  .msg { color: var(--danger); font-size: 12px; }
  .scroll { overflow: auto; flex: 1; }
  table { border-collapse: collapse; width: 100%; }
  th { position: sticky; top: 0; background: var(--panel-2); text-align: left; padding: 4px 8px; font-weight: 600; font-size: 12px; border-bottom: 1px solid var(--line); white-space: nowrap; }
  td { padding: 3px 8px; border-bottom: 1px solid var(--line); vertical-align: middle; }
  tr:hover td { background: var(--panel-2); }
  tr.sel td { background: var(--accent-bg); box-shadow: inset 0 1px 0 var(--accent), inset 0 -1px 0 var(--accent); }
  tr.sel td:first-child { box-shadow: inset 3px 0 0 var(--accent), inset 0 1px 0 var(--accent), inset 0 -1px 0 var(--accent); }
  .st { font-weight: 600; white-space: nowrap; }
  .num { white-space: nowrap; color: var(--muted); }
  .note { max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--muted); }
  td input { width: 160px; }
  .kinds { white-space: nowrap; }
  .badge { display: inline-block; margin-right: 3px; padding: 0 6px; border-radius: 4px; font-size: 11px; background: var(--panel-2); border: 1px solid var(--line); }
  .badge.lead { border-color: var(--warn); color: var(--warn); }
  .badge.openEnd { border-color: var(--accent); color: var(--accent); }
  .badge.flagged { border-color: var(--danger); color: var(--danger); }
  .hint { margin-left: 4px; font-size: 11px; padding: 0 5px; border: 1px dashed var(--warn); border-radius: 4px; background: none; color: var(--warn); cursor: pointer; }
  .none { color: var(--muted); padding: 14px; }
</style>
