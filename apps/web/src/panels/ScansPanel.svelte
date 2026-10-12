<script lang="ts">
  // The Scans panel: the person's scanned field-book pages and sketches, browsed from a folder on this
  // machine and shown large with zoom and pan. A scan can be linked to a trip: the link is just its path
  // (kept in the cave, undoable here), and the Notebook and this panel share the current trip, so turning
  // to a trip's page brings up its scan and picking a linked scan turns the notebook to its trip. The
  // pictures are never uploaded and never go into the cave.
  import { untrack } from 'svelte';
  import { ChevronDown, ChevronRight, FolderOpen, Folder, Link2, Unlink2, BookOpen } from '@lucide/svelte';
  import PanelUndo from '../components/PanelUndo.svelte';
  import ScanViewer from '../components/ScanViewer.svelte';
  import { cave, changed } from '../lib/cave.svelte';
  import { engine, loadEngine } from '../lib/engine';
  import type { TripChoice } from '../lib/notebook';
  import { treeRows } from '../lib/scan-tree';
  import { addFiles, checkRemembered, have, pickFolder, reopenRemembered, scans, select, toggleFolder } from '../lib/scans.svelte';
  import { showPanel } from '../lib/workspace.svelte';

  let ready = $state(false);
  let trips = $state<TripChoice[]>([]);
  let links = $state<{ trip: string; path: string }[]>([]);
  let filter = $state('');
  let over = $state(false);
  loadEngine().then(() => (ready = true));
  checkRemembered();

  $effect(() => {
    void cave.version;
    void cave.opened;
    void ready;
    untrack(() => {
      if (!ready) return;
      trips = engine.notebookTrips();
      links = engine.scanLinks();
      if (!trips.some((t) => t.id === cave.trip)) cave.trip = trips[0]?.id ?? '';
    });
  });

  const rows = $derived(treeRows(scans.paths, new Set(scans.collapsed), filter));
  const mine = $derived(links.filter((l) => l.trip === cave.trip).map((l) => l.path));
  const tripsOf = (path: string) => links.filter((l) => l.path === path).map((l) => l.trip);
  const tripNumber = (id: string) => (trips.find((t) => t.id === id)?.index ?? 0) + 1;
  const linkedHere = $derived(scans.selected !== '' && mine.includes(scans.selected));

  // Turning to a trip brings up its first scan, unless what is on show already belongs to it.
  $effect(() => {
    const trip = cave.trip;
    untrack(() => {
      if (!trip || mine.includes(scans.selected)) return;
      const first = links.find((l) => l.trip === trip && have(l.path));
      if (first) select(first.path);
    });
  });

  // Picking a scan that belongs to exactly one trip turns the notebook to that trip.
  $effect(() => {
    const path = scans.selected;
    untrack(() => {
      const owners = path ? tripsOf(path) : [];
      if (owners.length === 1 && owners[0] !== cave.trip) cave.trip = owners[0];
    });
  });

  function toggleLink() {
    if (!scans.selected || !cave.trip) return;
    const r = linkedHere ? engine.scanUnlink(cave.trip, scans.selected) : engine.scanLink(cave.trip, scans.selected);
    scans.message = r.startsWith('error') ? r.replace(/^error: /, '') : '';
    changed();
  }

  async function open() {
    await pickFolder();
  }

  function drop(e: DragEvent) {
    e.preventDefault();
    over = false;
    const list = [...(e.dataTransfer?.files ?? [])];
    if (list.length) addFiles(list);
  }

  function onKey(e: KeyboardEvent) {
    const t = e.target as HTMLElement;
    if (t.tagName === 'INPUT' || t.tagName === 'SELECT') return;
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      if (e.shiftKey ? engine.redo('scans') : engine.undo('scans')) changed();
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions: the panel takes dropped files and Ctrl/Cmd+Z for its own undo -->
<div class="panel" class:over role="region" aria-label="Scans" tabindex="-1" onkeydown={onKey} ondragover={(e) => { e.preventDefault(); over = true; }} ondragleave={() => (over = false)} ondrop={drop}>
  <div class="bar">
    <PanelUndo scope="scans" label="in the scans" />
    <button onclick={open} title="Pick the folder your scans are in. They are read here and never uploaded."><FolderOpen size={14} /> Open folder</button>
    {#if scans.remembered}<button onclick={reopenRemembered} title="Read the folder you used last time again">Reopen “{scans.remembered}”</button>{/if}
    <label>Trip
      <select value={cave.trip} aria-label="Trip" onchange={(e) => (cave.trip = e.currentTarget.value)}>
        {#each trips as t}<option value={t.id}>{t.index + 1}. {t.name || 'Untitled trip'}{t.date ? ` (${t.date})` : ''}</option>{/each}
      </select>
    </label>
    <button onclick={toggleLink} disabled={!scans.selected || !cave.trip} title={linkedHere ? 'Take this scan off the trip' : 'Link this scan to the trip, so the notebook page and the scan stay together'}>
      {#if linkedHere}<Unlink2 size={14} /> Unlink{:else}<Link2 size={14} /> Link to trip{/if}
    </button>
    <button onclick={() => showPanel('notebook')} title="Show the notebook page for this trip"><BookOpen size={14} /> Notebook</button>
    {#if scans.message}<span class="msg" role="status">{scans.message}</span>{/if}
  </div>

  <div class="body">
    <div class="side">
      {#if mine.length}
        <div class="head">This trip's scans</div>
        {#each mine as p}
          <button class="file" class:sel={scans.selected === p} class:gone={!have(p)} title={have(p) ? p : `${p} is not in the open folder`} onclick={() => select(p)}>{p.slice(p.lastIndexOf('/') + 1)}</button>
        {/each}
      {/if}
      <div class="head">{scans.folder || 'Scans'} {#if scans.paths.length}<span class="n">{scans.paths.length}</span>{/if}</div>
      {#if scans.paths.length}
        <input type="search" placeholder="Find a scan" bind:value={filter} aria-label="Find a scan" />
      {/if}
      <div class="tree" role="tree">
        {#each rows as r (r.path)}
          {#if r.kind === 'folder'}
            <button class="folder" style="padding-left: {6 + r.depth * 14}px" role="treeitem" aria-selected="false" aria-expanded={!r.collapsed} onclick={() => toggleFolder(r.path)}>
              {#if r.collapsed}<ChevronRight size={13} />{:else}<ChevronDown size={13} />{/if}<Folder size={13} /> {r.name} <span class="n">{r.count}</span>
            </button>
          {:else}
            <button class="file" class:sel={scans.selected === r.path} style="padding-left: {20 + r.depth * 14}px" role="treeitem" aria-selected={scans.selected === r.path} title={r.path} onclick={() => select(r.path)}>
              {r.name}{#each tripsOf(r.path) as t}<span class="chip" title="Linked to trip {tripNumber(t)}">{tripNumber(t)}</span>{/each}
            </button>
          {/if}
        {:else}
          <p class="none">{scans.paths.length ? 'No scan matches.' : 'Open the folder your scans are in, or drop scan files here. They stay on this machine.'}</p>
        {/each}
      </div>
    </div>
    {#if scans.url}
      {#key scans.url}<ScanViewer url={scans.url} kind={scans.kind} name={scans.selected} />{/key}
    {:else}
      <div class="empty">{scans.paths.length ? 'Pick a scan to see it.' : ''}</div>
    {/if}
  </div>
</div>

<style>
  .panel { display: flex; flex-direction: column; height: 100%; background: var(--panel); color: var(--text); font-size: 13px; outline: none; }
  .panel.over { box-shadow: inset 0 0 0 2px var(--accent); }
  .bar { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 6px 10px; border-bottom: 1px solid var(--line); }
  .bar label { display: inline-flex; align-items: center; gap: 4px; color: var(--muted); font-size: 12px; }
  button { display: inline-flex; align-items: center; gap: 4px; font: inherit; font-size: 12px; padding: 3px 8px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  input, select { font: inherit; font-size: 12px; padding: 3px 6px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); min-width: 0; }
  .msg { color: var(--muted); font-size: 12px; }
  .body { flex: 1; min-height: 0; display: flex; }
  .side { width: 220px; flex: none; display: flex; flex-direction: column; gap: 2px; padding: 6px; border-right: 1px solid var(--line); overflow: auto; }
  .head { font-size: 11px; font-weight: 600; color: var(--muted); padding: 6px 4px 2px; }
  .n { color: var(--muted); font-weight: 400; margin-left: 4px; font-size: 11px; }
  .tree { display: flex; flex-direction: column; }
  .side button.folder, .side button.file { border: none; background: none; text-align: left; padding: 3px 6px; border-radius: 4px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .side button.folder { font-weight: 600; }
  .side button:hover { background: var(--panel-2); }
  .side button.sel { background: var(--accent-bg); }
  .side button.gone { color: var(--muted); text-decoration: line-through; }
  .chip { margin-left: 6px; padding: 0 5px; border-radius: 999px; border: 1px solid var(--line); font-size: 10px; color: var(--muted); }
  .none { color: var(--muted); font-size: 12px; padding: 8px 4px; }
  .empty { flex: 1; display: grid; place-items: center; color: var(--muted); }
</style>
