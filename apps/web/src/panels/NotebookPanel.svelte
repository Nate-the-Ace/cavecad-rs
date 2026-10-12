<script lang="ts">
  // The Survey Notebook: one trip laid out the way a cave survey notes page is. Stations run down the
  // left with their passage dimensions (L R U D) beside them; each shot's tape, compass and clino
  // (foresight and backsight) sit on the line between the two stations. A gap is a chain breaking: the
  // station written after it is where the next shots start (a branch); a splay is `<station>.<n>`.
  // Compass cells hold what the compass read (magnetic); the trip's declination turns them into the true
  // bearings the survey keeps. The page is worked out from the survey every time and every cell is a real,
  // undoable edit of it (this panel has its own Undo and Redo).
  import { untrack } from 'svelte';
  import { Plus, ScanLine, Trash2 } from '@lucide/svelte';
  import { showPanel } from '../lib/workspace.svelte';
  import PanelUndo from '../components/PanelUndo.svelte';
  import { cave, changed, setPick } from '../lib/cave.svelte';
  import { engine, loadEngine } from '../lib/engine';
  import { SIDES, SIDE_LETTER, angleText, lrudText, parseAngle, parseLrud, suggestNext, withSide, type NotebookPage, type NotebookRow, type Side, type TripChoice } from '../lib/notebook';
  import { formatLength, lengthText, parseLength, unitName } from '../lib/units';

  let ready = $state(false);
  let trips = $state<TripChoice[]>([]);
  let page = $state<NotebookPage | null>(null);
  let message = $state('');
  let from = $state('');
  let to = $state('');
  /** Bumped to redraw cells after a refused edit, so a rejected entry does not stay on screen. */
  let redraw = $state(0);
  loadEngine().then(() => (ready = true));

  $effect(() => {
    void cave.version;
    void cave.opened;
    void cave.trip;
    void ready;
    untrack(() => {
      if (!ready) return;
      trips = engine.notebookTrips();
      if (!trips.some((t) => t.id === cave.trip)) cave.trip = trips[0]?.id ?? '';
      // Cells are updated in place (not rebuilt) so the focus stays where it is and Tab goes to the next cell.
      page = cave.trip ? engine.notebookPage(cave.trip) : null;
    });
  });

  // The next shot starts where the page ends and goes to the next station in the sequence, the usual
  // case: both are filled in (and can be changed). A name typed in To is left alone.
  let toTyped = false;
  $effect(() => {
    const last = [...(page?.rows ?? [])].reverse().find((r) => r.kind === 'station');
    from = last?.label ?? '';
    toTyped = false;
  });
  $effect(() => {
    const f = from;
    void page; // taken names change when a shot is added
    void ready;
    untrack(() => {
      if (toTyped || !ready) return; // the engine may not be loaded yet
      const taken = new Set(engine.scanViewStations('map', false).map((s) => s.label));
      for (const r of page?.rows ?? []) taken.add(r.label);
      to = suggestNext(f, taken);
    });
  });

  const units = $derived(cave.units);

  /** Runs an engine edit, shows what is wrong in plain words, and refreshes. */
  function run(result: string): boolean {
    const bad = result.startsWith('error');
    message = bad ? result.replace(/^error: /, '') : '';
    if (!bad) changed();
    else redraw += 1;
    return !bad;
  }

  type Which = 'distance' | 'az_fs' | 'az_bs' | 'inc_fs' | 'inc_bs';

  function reading(row: NotebookRow, which: Which, text: string) {
    if (!row.shot) return;
    let v: number | null | undefined;
    if (which === 'distance') {
      v = text.trim() === '' ? null : parseLength(text, units);
      if (v !== null && v !== undefined && v < 0) v = undefined;
    } else v = parseAngle(text, which.startsWith('az') ? 'compass' : 'clino');
    if (v === undefined) {
      message = which === 'distance' ? 'Not a length.' : which.startsWith('az') ? 'A compass reading is 0 to 360.' : 'A clino reading is -90 to 90.';
      redraw += 1;
      return;
    }
    run(engine.notebookSetReading(row.shot, which, v));
  }

  function side(row: NotebookRow, s: Side, text: string) {
    const target = row.shot ?? row.station;
    if (!target || !row.lrud) return;
    const p = parseLrud(text, units);
    if (p === undefined) {
      message = 'A dimension is a length, or P for open passage.';
      redraw += 1;
      return;
    }
    run(engine.notebookSetWalls(target, withSide(row.lrud, s, p.value, p.open)));
  }

  const note = (row: NotebookRow, text: string) => row.note_source && run(engine.notebookSetNote(row.note_source, text));
  const rename = (row: NotebookRow, text: string) => row.station && run(engine.notebookRenameStation(row.station, text));
  const field = (f: 'name' | 'date' | 'team' | 'instruments', text: string) => page && run(engine.notebookSetTripField(page.trip, f, text));

  function declination(text: string) {
    if (!page) return;
    const v = parseFloat(text);
    if (!Number.isFinite(v)) {
      message = 'Declination is degrees, east positive.';
      redraw += 1;
      return;
    }
    run(engine.notebookSetDeclination(page.trip, v));
  }

  function add(splay: boolean) {
    if (!page) return;
    const target = to.trim();
    if (run(engine.notebookAddShot(page.trip, from, splay ? '' : target, splay))) toTyped = false;
  }

  function newTrip() {
    const r = engine.notebookNewTrip(`Trip ${trips.length + 1}`);
    if (r.startsWith('error')) return void run(r);
    cave.trip = r;
    changed();
  }

  function pick(row: NotebookRow) {
    if (row.kind === 'station' && row.station) setPick('station', row.station);
    else if ((row.kind === 'shot' || row.kind === 'splay') && row.shot) setPick(row.kind === 'splay' ? 'splay' : 'leg', row.shot);
  }

  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (t.tagName === 'INPUT' || t.tagName === 'SELECT') return; // typing: leave the keys to the field
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      if (e.shiftKey ? engine.redo('notebook') : engine.undo('notebook')) changed();
    }
  }

  const enter = (e: KeyboardEvent) => e.key === 'Enter' && (e.currentTarget as HTMLInputElement).blur();
  const closureRow = (i: number) => document.getElementById(`nb-row-${i}`)?.scrollIntoView({ block: 'center' });
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions: the panel takes Ctrl/Cmd+Z for its own undo -->
<div class="panel" role="region" aria-label="Survey notebook" tabindex="-1" onkeydown={onKey}>
  <div class="bar">
    <PanelUndo scope="notebook" label="in the notebook" />
    <select value={cave.trip} aria-label="Trip" onchange={(e) => (cave.trip = e.currentTarget.value)}>
      {#each trips as t}<option value={t.id}>{t.index + 1}. {t.name || 'Untitled trip'}{t.date ? ` (${t.date})` : ''}</option>{/each}
    </select>
    <button class="icon" title="Start a new trip (a new page)" aria-label="New trip" onclick={newTrip}><Plus size={15} /></button>
    {#if page}
      <button class="scan" onclick={() => showPanel('scans')} title="Show this trip's scanned pages beside the notes"><ScanLine size={14} /> Scans</button>
      {#key redraw}
        <label>Name <input class="wide" value={page.name} maxlength="200" onchange={(e) => field('name', e.currentTarget.value)} onkeydown={enter} /></label>
        <label>Date <input class="date" value={page.date} placeholder="YYYY-MM-DD" onchange={(e) => field('date', e.currentTarget.value)} onkeydown={enter} /></label>
        <label>Team <input class="wide" value={page.team} maxlength="200" onchange={(e) => field('team', e.currentTarget.value)} onkeydown={enter} /></label>
        <label>Instruments <input value={page.instruments} maxlength="200" onchange={(e) => field('instruments', e.currentTarget.value)} onkeydown={enter} /></label>
        <label title="Magnetic declination, degrees east. Changing it turns every bearing in this trip; the compass cells keep what was read.">Decl. <input class="num" value={page.declination ?? ''} onchange={(e) => declination(e.currentTarget.value)} onkeydown={enter} />°</label>
      {/key}
    {/if}
    {#if message}<span class="msg" role="alert">{message}</span>{/if}
  </div>

  {#if page}
    <div class="totals" role="status">
      <b>{formatLength(page.totals.length, units, 1)}</b> surveyed · {page.totals.shots} shots · {page.totals.splays} splays · {page.totals.stations} stations
      {#if page.totals.warnings}<span class="warn">· {page.totals.warnings} to check</span>{/if}
      <span class="note">Notes written in {page.unit === 'ft' ? 'feet' : 'metres'}; shown in {unitName(units) === 'ft' ? 'feet' : 'metres'}. Compass cells are magnetic.</span>
    </div>
    <div class="scroll">
      <table>
        <thead>
          <tr>
            <th class="st">Station</th><th>Tape ({unitName(units)})</th><th>Az fs</th><th>Az bs</th><th>Inc fs</th><th>Inc bs</th><th>Total</th>
            {#each SIDES as s}<th class="lr" title={s}>{SIDE_LETTER[s]}</th>{/each}
            <th class="notes">Notes</th><th></th>
          </tr>
        </thead>
        {#key redraw}
          <tbody>
            {#each page.rows as r, i (i)}
              {#if r.kind === 'gap'}
                <tr id="nb-row-{i}" class="gap"><td colspan="13"></td></tr>
              {:else if r.kind === 'station'}
                <tr id="nb-row-{i}" class="station" class:sel={cave.pick.kind === 'station' && cave.pick.id === r.station} onclick={() => pick(r)}>
                  <td class="st"><input value={r.label} tabindex="-1" title="Click to rename the station (Tab skips it, so entry flows tape, compass, clino, then L R U D)" aria-label="Station name" maxlength="40" onchange={(e) => rename(r, e.currentTarget.value)} onkeydown={enter} />{#if r.anchor}<span class="tag" title="Where a chain starts; nothing on this page arrives here">start</span>{/if}</td>
                  <td colspan="6"></td>
                  {#each SIDES as s}
                    <td class="lr">
                      {#if r.lrud && r.lrud_editable}
                        <input class="num" value={lrudText(r.lrud[s], r.lrud[`${s}_open`], units)} aria-label="{s} of {r.label}" onchange={(e) => side(r, s, e.currentTarget.value)} onkeydown={enter} />
                      {:else if r.lrud}
                        <span class="ro" title="Recorded on another trip's page">{lrudText(r.lrud[s], r.lrud[`${s}_open`], units)}</span>
                      {/if}
                    </td>
                  {/each}
                  <td class="notes">{#if r.note_source}<input value={r.note} maxlength="400" aria-label="Note at {r.label}" onchange={(e) => note(r, e.currentTarget.value)} onkeydown={enter} />{/if}</td>
                  <td></td>
                </tr>
              {:else}
                <tr id="nb-row-{i}" class="shot" class:splay={r.kind === 'splay'} class:out={r.excluded} class:bad={r.warn !== ''} class:sel={(cave.pick.kind === 'leg' || cave.pick.kind === 'splay') && cave.pick.id === r.shot} title={r.warn} onclick={() => pick(r)}>
                  <td class="st">{#if r.kind === 'splay'}<span class="splay-name">{r.label}</span>{/if}</td>
                  <td><input class="num" value={r.distance === null ? '' : lengthText(r.distance, units)} aria-label="Tape" onchange={(e) => reading(r, 'distance', e.currentTarget.value)} onkeydown={enter} /></td>
                  <td><input class="num" value={angleText(r.az_fs)} aria-label="Compass foresight" onchange={(e) => reading(r, 'az_fs', e.currentTarget.value)} onkeydown={enter} /></td>
                  <td><input class="num" value={angleText(r.az_bs)} aria-label="Compass backsight" onchange={(e) => reading(r, 'az_bs', e.currentTarget.value)} onkeydown={enter} /></td>
                  <td><input class="num" value={angleText(r.inc_fs)} aria-label="Clino foresight" onchange={(e) => reading(r, 'inc_fs', e.currentTarget.value)} onkeydown={enter} /></td>
                  <td><input class="num" value={angleText(r.inc_bs)} aria-label="Clino backsight" onchange={(e) => reading(r, 'inc_bs', e.currentTarget.value)} onkeydown={enter} /></td>
                  <td class="num total">{r.total === null ? '' : formatLength(r.total, units, 1)}</td>
                  <td colspan="4"></td>
                  <td class="notes">{#if r.kind === 'splay' && r.note_source}<input value={r.note} maxlength="400" aria-label="Note on {r.label}" onchange={(e) => note(r, e.currentTarget.value)} onkeydown={enter} />{/if}</td>
                  <td><button class="icon" tabindex="-1" title="Delete this shot" aria-label="Delete shot" onclick={(e) => { e.stopPropagation(); if (r.shot) run(engine.notebookRemoveShot(r.shot)); }}><Trash2 size={13} /></button></td>
                </tr>
              {/if}
            {:else}
              <tr><td colspan="13" class="none">This trip has no shots yet. Add the first one below.</td></tr>
            {/each}
          </tbody>
        {/key}
      </table>
    </div>
    <div class="add">
      <label>From <input class="st-in" bind:value={from} maxlength="40" aria-label="Shot starts at" /></label>
      <label>To <input class="st-in" bind:value={to} oninput={() => (toTyped = true)} maxlength="40" aria-label="Shot ends at" onkeydown={(e) => e.key === 'Enter' && add(false)} /></label>
      <button onclick={() => add(false)} disabled={!from.trim() || !to.trim()}>Add shot</button>
      <button onclick={() => add(true)} disabled={!from.trim()} title="A reading to a wall: no station at the far end">Add splay</button>
      <span class="note">A station that already exists joins it (a loop); a different "From" starts a branch.</span>
    </div>
    {#if page.closures.length}
      <div class="closures" aria-label="Loop closures">
        {#each page.closures as c}
          <button class="closure" onclick={() => closureRow(c.row)} title={c.path.join(' → ')}>
            {c.kind === 'loop' ? 'Loop' : 'Tie'} {c.path[0]}–{c.path[c.path.length - 1]}: {formatLength(c.horizontal, units, 2)} off in plan{c.percent === null ? '' : `, ${c.percent.toFixed(1)}% of ${formatLength(c.length, units, 0)}`}
          </button>
        {/each}
      </div>
    {/if}
  {:else}
    <p class="none">Open a cave to see its notebook.</p>
  {/if}
</div>

<style>
  .panel { display: flex; flex-direction: column; height: 100%; background: var(--panel); color: var(--text); font-size: 13px; outline: none; }
  .bar { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 6px 10px; border-bottom: 1px solid var(--line); }
  .bar label { display: inline-flex; align-items: center; gap: 4px; color: var(--muted); font-size: 12px; }
  input, select { font: inherit; font-size: 12px; padding: 3px 6px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); min-width: 0; }
  .wide { width: 150px; }
  .date { width: 100px; }
  input.num { width: 4.6em; text-align: right; }
  .msg { color: var(--danger); font-size: 12px; }
  .totals { padding: 4px 10px; border-bottom: 1px solid var(--line); font-size: 12px; }
  .totals .warn { color: var(--warn); }
  .note { color: var(--muted); font-size: 11px; margin-left: 8px; }
  .scroll { flex: 1; overflow: auto; }
  table { border-collapse: collapse; width: 100%; }
  th { position: sticky; top: 0; background: var(--panel); text-align: left; font-weight: 600; font-size: 11px; color: var(--muted); padding: 4px 4px; border-bottom: 1px solid var(--line); }
  td { padding: 1px 3px; }
  td.st, th.st { min-width: 110px; }
  td.st input { width: 80px; font-weight: 600; }
  td.notes input { width: 100%; }
  th.lr, td.lr { text-align: center; }
  tr.station td { background: var(--panel-2); border-top: 1px solid var(--line); }
  tr.shot td { padding-top: 0; }
  tr.shot.splay { color: var(--muted); }
  tr.shot.out { text-decoration: line-through; opacity: 0.6; }
  tr.shot.bad td.total { color: var(--warn); }
  tr.shot.bad input { border-color: var(--warn); }
  tr.gap td { height: 10px; background: repeating-linear-gradient(135deg, transparent 0 4px, var(--line) 4px 5px); }
  tr.sel td { outline: 1px solid var(--accent); outline-offset: -1px; }
  .splay-name { display: inline-block; margin-left: 22px; padding-left: 8px; border-left: 2px solid var(--line); }
  .splay-name, .tag { font-size: 11px; color: var(--muted); margin-left: 4px; }
  .ro { color: var(--muted); font-size: 12px; }
  .total { color: var(--muted); font-size: 12px; text-align: right; white-space: nowrap; }
  .add { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 6px 10px; border-top: 1px solid var(--line); }
  .add label { display: inline-flex; align-items: center; gap: 4px; color: var(--muted); font-size: 12px; }
  .st-in { width: 90px; }
  .closures { display: flex; flex-wrap: wrap; gap: 6px; padding: 4px 10px 8px; border-top: 1px solid var(--line); max-height: 90px; overflow: auto; }
  .closure { padding: 2px 8px; border-radius: 999px; border: 1px solid var(--line); background: none; color: var(--text); cursor: pointer; font-size: 12px; }
  .none { padding: 14px; color: var(--muted); }
  button.scan { display: inline-flex; align-items: center; gap: 4px; font: inherit; font-size: 12px; padding: 3px 8px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); cursor: pointer; }
  button.icon { background: none; border: none; color: var(--muted); cursor: pointer; padding: 2px; }
  button.icon:hover { color: var(--text); }
</style>
