<script lang="ts">
  // The Scans panel: the person's scanned field-book pages and sketches, browsed from a folder on this
  // machine and shown large with zoom and pan. A scan can be linked to a trip: the link is just its path
  // (kept in the cave, undoable here), and the Notebook and this panel share the current trip, so turning
  // to a trip's page brings up its scan and picking a linked scan turns the notebook to its trip. The
  // pictures are never uploaded and never go into the cave.
  import { untrack } from 'svelte';
  import { ChevronDown, ChevronRight, FolderOpen, Folder, Link2, Unlink2, BookOpen, Check, Crosshair, Scissors, X } from '@lucide/svelte';
  import PanelUndo from '../components/PanelUndo.svelte';
  import ScanViewer from '../components/ScanViewer.svelte';
  import { cave, changed, clearPick, option, setOption, setPick } from '../lib/cave.svelte';
  import { engine, loadEngine, type ScanAlignment } from '../lib/engine';
  import { formatLength } from '../lib/units';
  import type { TripChoice } from '../lib/notebook';
  import { countDone, filesUnder, foldersOf, treeRows, withoutComplete, type TreeRow } from '../lib/scan-tree';
  import { addFiles, align, dropItems, trimming, checkRemembered, have, forgetCopy, pickFolder, revealReal, realRoot, setRealRoot, reopenRemembered, scans, select, setFolders, toggleFolder } from '../lib/scans.svelte';
  import { showPanel } from '../lib/workspace.svelte';
  import { isDesktop } from '../lib/desktop';
  import { cleanRoot, copyPathHint, fullPath, osOf, revealHint, revealLabel } from '../lib/scan-location';

  let ready = $state(false);
  let trips = $state<TripChoice[]>([]);
  let links = $state<{ trip: string; path: string }[]>([]);
  let filter = $state('');
  let over = $state(false);
  let al = $state<ScanAlignment | null>(null);
  /** Pages marked complete, and whether the tree hides them (to see what still needs attention). */
  let done = $state<Set<string>>(new Set());
  let hideDone = $state(false);
  /** Width of the tree, dragged by the person and remembered on this machine. */
  const WIDTH_KEY = 'cavecad.scans.treeWidth';
  let treeWidth = $state(
    (() => {
      try {
        const n = Number(localStorage.getItem(WIDTH_KEY));
        return Number.isFinite(n) && n >= 120 ? n : 220;
      } catch {
        return 220;
      }
    })(),
  );
  let body = $state<HTMLDivElement>();

  /** Sets the tree's width, kept between 120 px and most of the panel. */
  function setTreeWidth(w: number, save = true) {
    const max = Math.max(160, (body?.clientWidth ?? 600) - 160);
    treeWidth = Math.round(Math.min(Math.max(w, 120), max));
    if (!save) return;
    try {
      localStorage.setItem(WIDTH_KEY, String(treeWidth));
    } catch {
      /* not remembered */
    }
  }

  function dragTree(e: PointerEvent) {
    e.preventDefault();
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const left = body!.getBoundingClientRect().left;
    const move = (m: PointerEvent) => setTreeWidth(m.clientX - left, false);
    const up = () => {
      handle.removeEventListener('pointermove', move);
      handle.removeEventListener('pointerup', up);
      setTreeWidth(treeWidth);
    };
    handle.addEventListener('pointermove', move);
    handle.addEventListener('pointerup', up);
  }

  function nudgeTree(e: KeyboardEvent) {
    if (e.key === 'ArrowLeft') setTreeWidth(treeWidth - 20);
    else if (e.key === 'ArrowRight') setTreeWidth(treeWidth + 20);
    else return;
    e.preventDefault();
  }
  const os = osOf(navigator.platform ?? '', navigator.userAgent ?? '');
  /** Revealing a page or folder: asking where the scans folder lives, and the path handed back. */
  let asking = $state<{ path: string } | null>(null);
  let rootText = $state('');
  let revealed = $state<{ full: string; copied: boolean } | null>(null);
  /** The right-click menu: what it is for and where it is. */
  let menu = $state<{ row: TreeRow | { kind: 'file'; path: string; name: string; depth: number; count: number; collapsed: boolean }; x: number; y: number } | null>(null);
  let stations = $state<{ id: string; station: string }[]>([]);
  let chosen = $state('');
  let placed = $state<{ used: number; rms: number } | null>(null);
  let savedTrim = $state<[number, number][] | null>(null);
  loadEngine().then(() => (ready = true));
  checkRemembered();

  $effect(() => {
    void cave.version;
    void cave.opened;
    void align.view; // what the panel shows is read again for the new view
    void ready;
    untrack(() => {
      if (!ready) return;
      trips = engine.notebookTrips();
      links = engine.scanLinks();
      if (!trips.some((t) => t.id === cave.trip)) cave.trip = trips[0]?.id ?? '';
      done = new Set(engine.scanCompleted());
      stations = engine.scanViewStations(align.view, cave.adjusted).map((r) => ({ id: r.id, station: r.label }));
      al = scans.selected && cave.trip ? engine.scanAlignment(cave.trip, align.view, scans.selected) : null;
      const p = engine.scanPlacements(align.view, cave.adjusted).find((x) => x.trip === cave.trip && x.path === scans.selected);
      placed = p ? { used: p.used, rms: p.rms } : null;
      savedTrim = scans.selected && cave.trip ? engine.scanTrim(cave.trip, align.view, scans.selected) : null;
    });
  });

  const allRows = $derived(treeRows(scans.paths, new Set(scans.collapsed), filter));
  const rows = $derived(hideDone ? withoutComplete(allRows, done, scans.paths) : allRows);
  const totalDone = $derived(countDone(scans.paths, done));
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

  // Which drawing the scan is being placed in follows the run and station on show in the Profile and Cross Section.
  $effect(() => {
    const v = align.kind === 'profile' ? (cave.run ? `profile:${cave.run}` : 'map') : align.kind === 'section' ? (cave.station ? `section:${cave.station}` : 'map') : 'map';
    if (v !== align.view) {
      align.view = v;
    }
  });

  // A place clicked in the Profile or Cross Section completes a match.
  let handledPoint = align.point.seq;
  $effect(() => {
    const pt = align.point;
    if (pt.seq === handledPoint) return;
    handledPoint = pt.seq;
    untrack(() => match(`${pt.x}:${pt.y}`));
  });

  // A different scan or trip: whatever was half done is dropped.
  $effect(() => {
    void scans.selected;
    void cave.trip;
    align.pending = null;
    trimming.draft = [];
  });

  /** A spot on the scan was clicked: it waits for its station (click it on the Map, or choose it here). */
  function picked(px: number, py: number, dir?: 'left' | 'right' | 'up' | 'down') {
    if (trimming.on) {
      trimming.draft = [...trimming.draft, [px, py]];
      return;
    }
    if (wallScale && align.pending && dir) {
      setScaleFromWall(px, py, dir);
      return;
    }
    align.pending = { px, py };
    chosen = '';
    clearPick();
  }

  /** Cross Section only: the first spot is the station, the second a wall straight out from it, so the passage's measurement there sets the scale. */
  const wallScale = $derived(align.on && align.kind === 'section' && align.wall && !!cave.station);
  const walls = $derived(wallScale ? (void cave.version, engine.stationDetail(cave.station, cave.adjusted)?.walls ?? null) : null);
  const wallNotes = $derived(
    Object.fromEntries((['left', 'right', 'up', 'down'] as const).map((d) => [d, walls?.[d] != null ? `${d} · ${formatLength(walls[d] as number, cave.units, 2)}` : `${d} · not measured`])) as Partial<Record<'left' | 'right' | 'up' | 'down', string>>,
  );

  function setScaleFromWall(px: number, py: number, dir: 'left' | 'right' | 'up' | 'down') {
    const first = align.pending;
    const v = walls?.[dir];
    if (!first) return;
    if (v == null || !(v > 0)) {
      scans.message = `No ${dir} measurement at this station: enter it in the Notebook, or pick another wall.`;
      return;
    }
    // a new scale replaces the old matches
    for (let n = al?.anchors.length ?? 0; n > 0; n--) engine.scanRemoveAnchor(cave.trip, align.view, scans.selected, 0);
    const there = dir === 'left' ? `${-v}:0` : dir === 'right' ? `${v}:0` : dir === 'up' ? `0:${v}` : `0:${-v}`;
    const a = engine.scanAddAnchor(cave.trip, align.view, scans.selected, first.px, first.py, cave.station);
    const b = engine.scanAddAnchor(cave.trip, align.view, scans.selected, px, py, there);
    scans.message = [a, b].find((r) => r.startsWith('error'))?.replace(/^error: /, '') ?? '';
    align.pending = null;
    changed();
  }

  /** Trims the scan to the outline traced (a dragged loop, or points closed with Enter). */
  function saveTrim(outline: [number, number][]) {
    const r = engine.scanSetTrim(cave.trip, align.view, scans.selected, outline);
    scans.message = r.startsWith('error') ? 'That outline is too small or not a shape.' : '';
    trimming.draft = [];
    if (!r.startsWith('error')) trimming.on = false;
    changed();
  }

  const closeOutline = () => trimming.draft.length >= 3 && saveTrim(trimming.draft);
  const clearTrim = () => saveTrim([]);
  function stopTrim() {
    trimming.on = false;
    trimming.draft = [];
  }

  function match(station: string) {
    const p = align.pending;
    if (!p || !station || !scans.selected) return;
    const r = engine.scanAddAnchor(cave.trip, align.view, scans.selected, p.px, p.py, station);
    scans.message = r.startsWith('error') ? r.replace(/^error: /, '') : '';
    align.pending = null;
    chosen = '';
    changed();
  }

  // The station clicked on the Map completes a match.
  $effect(() => {
    const pick = cave.pick;
    untrack(() => {
      if (align.on && align.pending && align.view === 'map' && pick.kind === 'station' && pick.id) match(pick.id);
    });
  });

  function unmatch(i: number) {
    engine.scanRemoveAnchor(cave.trip, align.view, scans.selected, i);
    changed();
  }

  const setMode = (warp: boolean) => {
    engine.scanSetMode(cave.trip, align.view, scans.selected, warp ? 'warp' : 'rigid');
    changed();
  };

  const showUnder = $derived(option('showScans', true));
  const opacity = $derived(option('scanOpacity', 0.5));

  /** The pages a menu entry acts on: the page itself, or everything inside a folder. */
  const targets = (r: TreeRow) => (r.kind === 'file' ? [r.path] : filesUnder(scans.paths, r.path));
  const allDone = (r: TreeRow) => {
    const t = targets(r);
    return t.length > 0 && countDone(t, done) === t.length;
  };

  function mark(r: TreeRow, complete: boolean) {
    const res = engine.scanSetComplete(targets(r), complete);
    scans.message = res.startsWith('error') ? res.replace(/^error: /, '') : '';
    menu = null;
    changed();
  }

  /** Folds or unfolds folders from the menu (`under` empty means the whole tree). */
  function fold(under: string, closed: boolean) {
    setFolders(foldersOf(scans.paths, under), closed);
    menu = null;
  }

  /** Hands back the real path of a page or folder (asking once where the folder lives), copied for pasting into Finder or Explorer. */
  async function reveal(path: string) {
    menu = null;
    if (isDesktop() && scans.root) {
      // The desktop app knows the real folder (it was captured when the folder was picked): open the file manager on it.
      void revealReal(path);
      return;
    }
    const root = realRoot(scans.folder);
    if (!root) {
      asking = { path };
      rootText = '';
      return;
    }
    const full = fullPath(root, path);
    let copied = false;
    try {
      await navigator.clipboard.writeText(full);
      copied = true;
    } catch {
      /* not allowed here: the path is shown to copy by hand */
    }
    revealed = { full, copied };
  }

  function saveRoot() {
    const root = cleanRoot(rootText);
    if (!root || !asking) {
      scans.message = 'That is not a full path (it should start with / or a drive letter).';
      return;
    }
    setRealRoot(scans.folder, root);
    const path = asking.path;
    asking = null;
    void reveal(path);
  }

  function openMenu(e: MouseEvent, r: TreeRow) {
    e.preventDefault();
    menu = { row: r, x: Math.min(e.clientX, window.innerWidth - 200), y: Math.min(e.clientY, window.innerHeight - 90) };
  }

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
    const items = e.dataTransfer?.items;
    if (items?.length) void dropItems(items);
    else {
      const list = [...(e.dataTransfer?.files ?? [])];
      if (list.length) addFiles(list);
    }
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && menu) {
      menu = null;
      return;
    }
    const t = e.target as HTMLElement;
    if (t.tagName === 'INPUT' || t.tagName === 'SELECT') return;
    if (trimming.on) {
      if (e.key === 'Enter') return void closeOutline();
      if (e.key === 'Escape') return stopTrim();
      if (e.key === 'Backspace') {
        e.preventDefault();
        trimming.draft = trimming.draft.slice(0, -1);
        return;
      }
    }
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      if (e.shiftKey ? engine.redo('scans') : engine.undo('scans')) changed();
    }
  }
</script>

{#if menu}
  <!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events: the veil only closes the menu (Escape does too) -->
  <div class="veil" onclick={() => (menu = null)} oncontextmenu={(e) => { e.preventDefault(); menu = null; }}></div>
  <div class="menu" role="menu" style="left: {menu.x}px; top: {menu.y}px">
    <div class="what">{menu.row.name}</div>
    {#if menu.row.kind === 'folder'}
      <button role="menuitem" onclick={() => menu && mark(menu.row as TreeRow, true)}>Mark all complete</button>
      <button role="menuitem" onclick={() => menu && mark(menu.row as TreeRow, false)}>Mark all incomplete</button>
      <hr />
      {#if menu.row.collapsed}
        <button role="menuitem" onclick={() => menu && fold(menu.row.path, false)}>Expand (this folder and inside)</button>
        <button role="menuitem" onclick={() => { if (menu) setFolders([menu.row.path], false); menu = null; }}>Expand this folder only</button>
      {:else}
        <button role="menuitem" onclick={() => menu && fold(menu.row.path, true)}>Collapse (this folder and inside)</button>
        <button role="menuitem" onclick={() => menu && fold(menu.row.path, false)}>Expand everything inside</button>
      {/if}
    {:else if done.has(menu.row.path)}
      <button role="menuitem" onclick={() => menu && mark(menu.row as TreeRow, false)}>Mark incomplete</button>
    {:else}
      <button role="menuitem" onclick={() => menu && mark(menu.row as TreeRow, true)}>Mark complete</button>
    {/if}
    <hr />
    <button role="menuitem" title="Shows the real location on your computer, not the copy this browser keeps" onclick={() => menu && reveal(menu.row.path)}>{revealLabel(os)}</button>
    <hr />
    <button role="menuitem" onclick={() => fold('', false)}>Expand all folders</button>
    <button role="menuitem" onclick={() => fold('', true)}>Collapse all folders</button>
  </div>
{/if}

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
    <button class:on={trimming.on} onclick={() => { trimming.on = !trimming.on; trimming.draft = []; if (trimming.on) align.on = false; }} disabled={!linkedHere} title={linkedHere ? 'Keep only the part of the scan you trace round (the sketch), so the rest does not clutter the views' : 'Link the scan to the trip first'}><Scissors size={14} /> Trim to outline</button>
    <button class:on={align.on} onclick={() => { align.on = !align.on; if (align.on) stopTrim(); }} disabled={!linkedHere} title={linkedHere ? 'Match spots on this scan to stations so it lies under the Map and you can trace over it' : 'Link the scan to the trip first'}><Crosshair size={14} /> Align under map</button>
    <label title="Draw aligned scans under the Map"><input type="checkbox" checked={showUnder} onchange={(e) => setOption('showScans', e.currentTarget.checked)} /> Show under map</label>
    <label title="How much of the scan shows through under the Map">Strength <input type="range" min="0.1" max="1" step="0.05" value={opacity} oninput={(e) => setOption('scanOpacity', +e.currentTarget.value)} /></label>
    {#if linkedHere}
      <label title="Which drawing the scan is placed in. Each has its own placement and outline, so one page can serve the Map, a Profile and a Cross Section.">Place in
        <select bind:value={align.kind} aria-label="Place the scan in">
          <option value="map">Map</option>
          <option value="profile" disabled={!cave.run}>Profile{cave.run ? ` (run ${cave.run})` : ''}</option>
          <option value="section" disabled={!cave.station}>Cross Section{cave.station ? ` (${stations.find((s) => s.id === cave.station)?.station ?? 'station'})` : ''}</option>
        </select>
      </label>
    {/if}
    <button onclick={() => showPanel('notebook')} title="Show the notebook page for this trip"><BookOpen size={14} /> Notebook</button>
    {#if scans.message}<span class="msg" role="status">{scans.message}</span>{/if}
  </div>

  {#if asking}
    <div class="alignbar" role="group" aria-label="Where the scans folder is">
      <span class="hint">
        Where is “{scans.folder}” on this computer? Paste its full path once and every page can be shown in its real place. {copyPathHint(os)}
      </span>
      <input class="rootbox" bind:value={rootText} placeholder={os === 'windows' ? 'C:\\Users\\you\\Documents\\scans' : '/Users/you/Documents/scans'} aria-label="Full path of the scans folder" onkeydown={(e) => e.key === 'Enter' && saveRoot()} />
      <button onclick={saveRoot}>Save and {revealLabel(os).toLowerCase()}</button>
      <button onclick={() => (asking = null)}>Cancel</button>
    </div>
  {/if}
  {#if revealed}
    <div class="alignbar" role="status">
      <span class="hint"><b>{revealed.copied ? 'Copied the real location:' : 'Real location (copy it):'}</b> <code>{revealed.full}</code><br />{revealHint(os)}</span>
      <button onclick={() => { asking = { path: '' }; rootText = realRoot(scans.folder); revealed = null; }} title="The folder moved, or the path was wrong">Change folder location</button>
      <button onclick={() => (revealed = null)}>Dismiss</button>
    </div>
  {/if}
  {#if trimming.on && linkedHere}
    <div class="alignbar" role="group" aria-label="Trim to an outline">
      <span class="hint">Drag a loop round the part to keep, or click its corners one by one, then close the outline.</span>
      <button onclick={closeOutline} disabled={trimming.draft.length < 3} title="Close the outline and trim (Enter)">Close outline</button>
      <button onclick={() => (trimming.draft = trimming.draft.slice(0, -1))} disabled={!trimming.draft.length} title="Take back the last corner (Backspace)">Undo corner</button>
      <button onclick={stopTrim} title="Stop without changing anything (Esc)">Cancel</button>
    </div>
  {:else if savedTrim && linkedHere}
    <div class="alignbar" role="status">
      <span class="hint">Trimmed to an outline: what is outside it is dimmed here and left out of the Map.</span>
      <button onclick={clearTrim}>Use the whole scan</button>
    </div>
  {/if}
  {#if align.on && linkedHere}
    <div class="alignbar" role="group" aria-label="Align under the map">
      <span class="hint">
        {#if wallScale}
          {#if align.pending}Now click the cave wall straight out from the station (left, right, up or down: the line snaps to the nearest). The passage's measurement there sets the scale. <button onclick={() => (align.pending = null)}>Cancel</button>
          {:else}Click the station's spot on the scan, then a point on the cave wall straight out from it.{/if}
        {:else if align.pending}
          {align.kind === 'map' ? `Now click that spot's station on the Map, or choose it:` : `Now click the same place in the ${align.kind === 'profile' ? 'Profile' : 'Cross Section'}, or choose its station:`}
          <select bind:value={chosen} aria-label="Station" onchange={() => match(chosen)}>
            <option value="">Station…</option>
            {#each stations as s}<option value={s.id}>{s.station}</option>{/each}
          </select>
          <button onclick={() => (align.pending = null)}>Cancel</button>
        {:else}
          Click a spot on the scan that you can find in the {align.kind === 'map' ? 'Map (a station mark)' : align.kind === 'profile' ? 'Profile (a station, or any place on it)' : 'Cross Section (the station, or any place on it)'}. {#if !al || al.anchors.length < 2}Two matches place and scale the scan; more fit it better. One match places it at the average scale of your other scans, north up.{/if}
        {/if}
      </span>
      {#if align.kind === 'section'}
        <label title="Click the station on the scan, then the wall straight out from it: the measured left, right, up or down at the station sets the scale. Turn off to match any two places by hand."><input type="checkbox" bind:checked={align.wall} onchange={() => (align.pending = null)} /> Set scale from a wall</label>
      {/if}
      {#each al?.anchors ?? [] as a, i}
        <span class="match">{a.label || `${formatLength(a.x ?? 0, cave.units, 1)}, ${formatLength(a.y ?? 0, cave.units, 1)}`}<button class="x" aria-label="Remove this match" onclick={() => unmatch(i)}><X size={11} /></button></span>
      {/each}
      {#if al && al.anchors.length >= 3}
        <label title="Also stretch and skew across and down to take out scanner distortion"><input type="checkbox" checked={al.mode === 'warp'} onchange={(e) => setMode(e.currentTarget.checked)} /> Stretch to fit</label>
      {/if}
      {#if placed}<span class="rms" title="How far, on average, the matched spots land from their stations">off by {formatLength(placed.rms, cave.units, 2)} on average</span>{/if}
    </div>
  {/if}
  <div class="body" bind:this={body}>
    <div class="side" style="width: {treeWidth}px">
      {#if mine.length}
        <div class="head">This trip's scans</div>
        {#each mine as p}
          <button class="file" class:sel={scans.selected === p} class:gone={!have(p)} title={have(p) ? p : `${p} is not in the open folder`} onclick={() => select(p)}>{p.slice(p.lastIndexOf('/') + 1)}</button>
        {/each}
      {/if}
      {#if scans.loading}<div class="kept" role="status">{scans.loading}</div>{/if}
      <div class="head">{scans.folder || 'Scans'} {#if scans.paths.length}<span class="n">{scans.paths.length}</span>{/if}</div>
      {#if scans.keptBytes}
        <div class="kept" title="This browser keeps a copy of your scans so a refresh does not lose them. It stays on this machine.">
          {scans.restored ? 'Kept from last time' : 'Copy kept in this browser'} · {(scans.keptBytes / 1048576).toFixed(0)} MB
          <button class="link" onclick={forgetCopy}>Forget it</button>
        </div>
      {/if}
      {#if scans.paths.length}
        <input type="search" placeholder="Find a scan" bind:value={filter} aria-label="Find a scan" />
        <label class="hide" title="Show only the pages that still need attention"><input type="checkbox" bind:checked={hideDone} /> Hide complete <span class="n">{totalDone}/{scans.paths.length}</span></label>
      {/if}
      <div class="tree" role="tree">
        {#each rows as r (r.path)}
          {#if r.kind === 'folder'}
            <button class="folder" class:complete={allDone(r)} style="padding-left: {6 + r.depth * 14}px" role="treeitem" aria-selected="false" aria-expanded={!r.collapsed} oncontextmenu={(e) => openMenu(e, r)} onclick={() => toggleFolder(r.path)}>
              {#if r.collapsed}<ChevronRight size={13} />{:else}<ChevronDown size={13} />{/if}<Folder size={13} /> {r.name}
              <span class="n" title="Pages marked complete / pages in this folder">{countDone(filesUnder(scans.paths, r.path), done)}/{r.count}</span>{#if allDone(r)}<Check size={12} class="ok" />{/if}
            </button>
          {:else}
            <button class="file" class:sel={scans.selected === r.path} class:complete={done.has(r.path)} style="padding-left: {20 + r.depth * 14}px" role="treeitem" aria-selected={scans.selected === r.path} title={r.path} oncontextmenu={(e) => openMenu(e, r)} onclick={() => select(r.path)}>
              {#if done.has(r.path)}<Check size={12} class="ok" />{/if}{r.name}{#each tripsOf(r.path) as t}<span class="chip" title="Linked to trip {tripNumber(t)}">{tripNumber(t)}</span>{/each}
            </button>
          {/if}
        {:else}
          <p class="none">{scans.paths.length ? 'No scan matches.' : 'Open the folder your scans are in, or drop the folder (or scan files) here. They stay on this machine.'}</p>
        {/each}
      </div>
    </div>
    <!-- svelte-ignore a11y_no_noninteractive_tabindex: the divider is a keyboard-adjustable separator -->
    <div class="split" role="separator" aria-orientation="vertical" aria-label="Resize the scan tree" aria-valuenow={treeWidth} tabindex="0" title="Drag to resize the tree (double-click to reset)" onpointerdown={dragTree} onkeydown={nudgeTree} ondblclick={() => setTreeWidth(220)}></div>
    {#if scans.url}
      {#key scans.url}<ScanViewer url={scans.url} kind={scans.kind} name={scans.selected} marks={align.on && linkedHere ? (al?.anchors ?? []).map((a) => ({ px: a.px, py: a.py, label: a.label || `${formatLength(a.x ?? 0, cave.units, 1)}, ${formatLength(a.y ?? 0, cave.units, 1)}` })) : []} pending={align.on ? align.pending : null} snapFrom={wallScale ? align.pending : null} snapNotes={wallNotes} picking={(align.on || trimming.on) && linkedHere} onpick={picked} trim={linkedHere ? savedTrim : null} draft={trimming.on ? trimming.draft : []} tracing={trimming.on && linkedHere} onstroke={saveTrim} />{/key}
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
  .split { flex: none; width: 5px; cursor: col-resize; background: var(--line); touch-action: none; }
  .split:hover, .split:focus-visible { background: var(--accent); outline: none; }
  .alignbar { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 5px 10px; border-bottom: 1px solid var(--line); background: var(--accent-bg); font-size: 12px; }
  .match { display: inline-flex; align-items: center; gap: 2px; padding: 0 3px 0 8px; border: 1px solid var(--line); border-radius: 999px; background: var(--panel); }
  .match .x { border: none; background: none; padding: 2px; }
  .rms { color: var(--muted); }
  button.on { background: var(--accent-bg); border-color: var(--accent); }
  .body { flex: 1; min-height: 0; display: flex; }
  .side { flex: none; display: flex; flex-direction: column; gap: 2px; padding: 6px; border-right: 1px solid var(--line); overflow: auto; }
  .side > * { flex: none; }
  .head { font-size: 11px; font-weight: 600; color: var(--muted); padding: 6px 4px 2px; }
  .n { color: var(--muted); font-weight: 400; margin-left: 4px; font-size: 11px; }
  .tree { display: flex; flex-direction: column; }
  .side button.folder, .side button.file { border: none; background: none; text-align: left; padding: 3px 6px; border-radius: 4px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .side button.folder { font-weight: 600; }
  .side button:hover { background: var(--panel-2); }
  .side button.sel { background: var(--accent-bg); }
  .side button.gone { color: var(--muted); text-decoration: line-through; }
  .chip { margin-left: 6px; padding: 0 5px; border-radius: 999px; border: 1px solid var(--line); font-size: 10px; color: var(--muted); }
  .side button.complete { color: var(--muted); }
  .side :global(.ok) { color: #34c759; flex: none; margin-right: 3px; }
  .hide { display: flex; align-items: center; gap: 4px; font-size: 11px; color: var(--muted); padding: 2px 4px; }
  .veil { position: fixed; inset: 0; z-index: 60; }
  .menu { position: fixed; z-index: 61; min-width: 170px; background: var(--panel); border: 1px solid var(--line); border-radius: 8px; padding: 4px; box-shadow: 0 6px 24px rgba(0, 0, 0, 0.35); display: flex; flex-direction: column; }
  .menu .what { font-size: 11px; color: var(--muted); padding: 4px 8px; max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .menu hr { border: none; border-top: 1px solid var(--line); margin: 4px 2px; width: 100%; }
  .menu button { border: none; background: none; text-align: left; padding: 6px 8px; border-radius: 4px; }
  .menu button:hover { background: var(--accent-bg); }
  .kept { font-size: 11px; color: var(--muted); padding: 0 4px 4px; }
  .kept .link { border: none; background: none; color: var(--accent); padding: 0 2px; font-size: 11px; cursor: pointer; text-decoration: underline; }
  .rootbox { flex: 1; min-width: 240px; font-family: var(--mono, monospace); }
  code { font-family: var(--mono, monospace); user-select: all; word-break: break-all; }
  .none { color: var(--muted); font-size: 12px; padding: 8px 4px; }
  .empty { flex: 1; display: grid; place-items: center; color: var(--muted); }
</style>
