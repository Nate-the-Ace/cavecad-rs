<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { Home, Maximize, Box, Square, Palette, Eye } from '@lucide/svelte';
  import PanelUndo from './PanelUndo.svelte';
  import { DEMOS } from '../lib/demos';
  import { isSymbolKind, symbolDef } from '../lib/symbols';
  import { confirmDiscard } from '../lib/files';
  import { cursorsFor, live as room, reportCursor } from '../lib/sync.svelte';
  import { leave } from '../lib/sync.svelte';
  import { engine, loadEngine, type Scene, type TypeStyle } from '../lib/engine';
  import { CaveView, type Projection } from '../lib/viewport/cave-view';
  import { cave, changed, filter, markSaved, option, setShowSections, setShown, wireFilter } from '../lib/cave.svelte';
  import { tolerancePx } from '../lib/commands';
  import { ViewCube } from '../lib/viewport/view-cube';
  import { forgetFile } from '../lib/files';
  import { onTheme } from '../lib/theme';
  import { panelState, showPanel } from '../lib/workspace.svelte';

  let wrap: HTMLDivElement;
  let gl: HTMLCanvasElement;
  let overlay: HTMLCanvasElement;
  let cubeHost: HTMLDivElement;
  let view: CaveView | null = null;

  /** The picker's value while the cave on screen came from a file, not a demo. */
  const FROM_FILE = '__file';
  let demo = $state(DEMOS[0].id);
  /** What the picker showed before the person changed it. */
  let shownDemo = DEMOS[0].id;
  let depthColor = $state(false);
  let walls = $state(true);
  let passage = $state(true);
  let fillGaps = $state(true);
  let projection = $state<Projection>('perspective');
  let status = $state('');
  let loaded = $state(false);
  let scene = $state.raw<Scene | null>(null);
  let palette = $state<TypeStyle[]>([]);
  let showMenu = $state(false);
  /** Palette entries the cave has something of, in palette order. */
  const listed = $derived(palette.filter((p) => scene?.types.includes(p.key)));
  /** The symbols the cave has, each listed under its own name. */
  const symbolTypes = $derived((scene?.types ?? []).filter(isSymbolKind).map((key) => ({ key, label: symbolDef(key)?.name ?? key })).sort((a, b) => a.label.localeCompare(b.label)));

  function open(id: string) {
    const d = DEMOS.find((x) => x.id === id);
    if (!d || !view) return;
    if (!confirmDiscard()) {
      demo = shownDemo; // declined: the picker goes back to what is on screen
      return;
    }
    shownDemo = id;
    leave(); // a different cave: stop sharing the old one
    status = engine.openDemo(d.json);
    markSaved();
    cave.fileName = `${d.id}.dxf`;
    forgetFile();
    show();
  }

  /** Draws whichever cave the engine has open, framed from the home view. */
  function show() {
    if (!view) return;
    cave.run = '';
    filter.hiddenTrips = []; // trip ids belong to one cave
    changed();
    scene = engine.scene(cave.adjusted, wireFilter());
    view.setScene(scene);
    view.setShowLabels(!filter.hiddenTypes.includes('label'));
    loadPassage();
  }

  $effect(() => {
    const u = cave.units; // read first so the effect always tracks it
    view?.setUnits(u);
  });

  // Cross-section sketches sit beside their stations and follow every edit, filter and toggle.
  $effect(() => {
    void cave.version;
    void cave.adjusted;
    void filter.hiddenTrips.length;
    const on = cave.showSections.map;
    const profileOn = cave.showSections.profileSketches;
    untrack(() => {
      view?.setShowSections(on);
      view?.setShowProfileSketches(profileOn);
      if (view && on) view.setSections(engine.mapSections(cave.adjusted, wireFilter()));
      if (view && profileOn) view.setProfileSketches(engine.mapProfileSketches(cave.adjusted));
      view?.setMapLines(engine.mapLines(cave.adjusted));
    });
  });

  // The selected map line is fitted again at another detail.
  let handledRefit = cave.refit.seq;
  $effect(() => {
    const r = cave.refit;
    if (r.seq === handledRefit) return;
    handledRefit = r.seq;
    untrack(() => view?.refitLine(r.id, r.detail));
  });

  // Other people's pointers on the plan.
  $effect(() => {
    for (const o of Object.values(room.cursors)) void o.seen;
    void room.people.length;
    const list = cursorsFor('map', '').map((o) => ({ x: o.x, y: o.y, name: o.person?.name ?? '', color: o.person?.color ?? '#888888' }));
    untrack(() => view?.setRemoteCursors(list));
  });

  // The selected line gets a halo and, on the map, handles; Delete removes a selected map line.
  $effect(() => {
    const id = cave.selected;
    view?.setSelectedLine(id);
  });
  /** Opens or closes the Show menu; opening re-reads what the cave has, so what was just drawn is listed. */
  function openMenu() {
    showMenu = !showMenu;
    if (showMenu) scene = engine.scene(cave.adjusted, wireFilter());
  }
  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape' && showMenu) {
      showMenu = false;
      return;
    }
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      if ((e.shiftKey ? engine.redo('map') : engine.undo('map'))) changed();
      return;
    }
    if ((e.key === 'Delete' || e.key === 'Backspace') && cave.selected && view?.hasMapLine(cave.selected)) {
      e.preventDefault();
      engine.deleteLine(cave.selected);
      cave.selected = '';
      changed();
    } else if (e.key === 'Escape' && cave.selected) {
      cave.selected = '';
    }
  }

  // How a stroke drawn on the map becomes a curve follows the running command's settings.
  $effect(() => {
    const tol = tolerancePx(option('detail', 5));
    const pressure = option('pressure', true);
    const close = option('close', false);
    const finger = option('finger', true);
    view?.setDrawOptions({ tolerancePx: tol, pressure, close, finger, symbolSize: option('symbolSize', 1.5), label: option('label', '') });
  });

  // The picked drawing tool: drawing on the map goes to its plan, from the Top or Bottom view.
  let drawHint = $state('');
  $effect(() => {
    const tool = cave.tool;
    view?.setDrawTool(tool);
    drawHint = '';
  });

  // The station the Cross Section panel shows is ringed on the map, while that panel can be seen.
  $effect(() => {
    const id = panelState.visible.section ? cave.station : '';
    view?.setSelected(id);
  });

  // A plain click on a station picks it for the Cross Section panel (a drag orbits, as always).
  let down: { x: number; y: number } | null = null;
  function onDown(e: PointerEvent) {
    showMenu = false; // a press on the map closes the Show menu
    wrap.focus({ preventScroll: true }); // so Delete and Escape reach the map
    down = e.button === 0 ? { x: e.clientX, y: e.clientY } : null;
  }
  function onUp(e: PointerEvent) {
    const d = down;
    down = null;
    if (!d || !view || cave.tool || Math.hypot(e.clientX - d.x, e.clientY - d.y) > 4) return;
    const hit = view.pickStation(e.clientX, e.clientY);
    if (!hit) return;
    cave.station = hit.id;
    showPanel('section');
    engine.log('info', 'section', `cross section at ${hit.label}`);
  }

  // A cave opened from a file replaces the one on screen.
  let seenOpen = 0;
  $effect(() => {
    const n = cave.opened;
    untrack(() => {
      if (n === seenOpen) return;
      seenOpen = n;
      demo = FROM_FILE;
      shownDemo = FROM_FILE;
      status = '';
      show();
    });
  });

  /** The passage surface follows the Adjusted and colour settings. */
  function loadPassage() {
    const buffers = engine.passageMesh(cave.adjusted, depthColor ? 'depth' : 'trip', fillGaps, wireFilter());
    view?.setPassage(buffers);
    if (buffers.filled > 0) {
      engine.log('info', 'passage', `${buffers.filled} unmeasured station${buffers.filled === 1 ? '' : 's'} filled in from the nearest measurements (shown paler)`);
    }
  }

  function refresh() {
    if (!view) return;
    scene = engine.scene(cave.adjusted, wireFilter());
    view.setShowLabels(!filter.hiddenTypes.includes('label'));
    view.setScene(scene, false);
    loadPassage();
    changed();
  }

  onMount(() => {
    const v = new CaveView(gl, overlay);
    const cube = new ViewCube(cubeHost, (dir) => v.snapTo(dir), (da, dp) => v.orbitBy(da, dp));
    v.onRender = (camera, target) => {
      cube.sync(camera, target);
      const hint = cave.tool && !v.canDrawHere ? 'Snap to the Top view to draw on the map' : '';
      if (hint !== drawHint) drawHint = hint;
    };
    v.onSelectLine = (id) => (cave.selected = id);
    v.onEndCommand = () => (cave.tool = '');
    v.onLineEdited = (id, points, widths) => {
      const r = engine.editLine(id, points, widths, cave.adjusted);
      if (r !== 'ok') engine.log('warn', 'draw', r);
      changed();
    };
    v.onCursor = (x, y) => reportCursor('map', '', x, y, cave.tool);
    v.onLineErased = (id) => {
      if (cave.selected === id) cave.selected = '';
      engine.deleteLine(id);
      changed();
    };
    v.onLineDrawn = (points, widths, closed, text = '') => {
      if (points.length === 0) {
        engine.log('warn', 'draw', 'A fill needs a loop: draw all the way around the area.');
        return;
      }
      if (cave.tool === 'text' && !text.trim()) {
        engine.log('warn', 'draw', 'Type the text in the ribbon first.');
        return;
      }
      const r = engine.addMapLine(cave.tool, points, widths, closed, cave.adjusted, text);
      if (r !== 'ok') engine.log('warn', 'draw', r);
      changed();
    };
    v.onProjection = (p) => (projection = p);
    v.setUnits(cave.units);
    v.onSectionPlaced = (id, offset) => {
      const r = engine.placeSection(id, 'map', offset);
      if (r !== 'ok') engine.log('warn', 'section', r);
      changed();
    };
    v.shadingFor = (dir, persp) => engine.shading(cave.adjusted, dir, persp, wireFilter());
    view = v;
    if (import.meta.env.DEV) (window as unknown as { __view: CaveView }).__view = v; // for tests

    const ro = new ResizeObserver(() => v.resize(wrap.clientWidth, wrap.clientHeight));
    ro.observe(wrap);
    const offTheme = onTheme(() => v.applyTheme());
    loadEngine().then(() => {
      loaded = true;
      palette = engine.palette();
      v.setPalette(palette);
      open(demo);
    });

    return () => {
      ro.disconnect();
      offTheme();
      cube.dispose();
      v.dispose();
      view = null;
    };
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="vp" role="application" aria-label="Map" bind:this={wrap} tabindex="-1" onkeydown={onKey}>
  <canvas bind:this={gl} onpointerdown={onDown} onpointerup={onUp}></canvas>
  <canvas bind:this={overlay} class="overlay"></canvas>
  <div class="bar">
    <select bind:value={demo} onchange={() => open(demo)} aria-label="Demo cave">
      {#if demo === FROM_FILE}<option value={FROM_FILE}>{cave.fileName}</option>{/if}
      {#each DEMOS as d}<option value={d.id}>{d.title}</option>{/each}
    </select>
    <div class="menu">
      <button class:on={showMenu || filter.hiddenTypes.length + filter.hiddenTrips.length > 0} title="Show or hide: walls, passage, sketches, sections, types and trips" aria-expanded={showMenu} onclick={openMenu}><Eye size={15} /> Show</button>
      {#if showMenu}
        <div class="pop" role="group" aria-label="Show or hide">
          <h4>On the map</h4>
          <label title="Show the survey with loop errors spread out (least-squares adjusted) instead of as measured"><input type="checkbox" bind:checked={cave.adjusted} onchange={refresh} /> Adjusted</label>
          <label><input type="checkbox" bind:checked={walls} onchange={() => view?.setShowWalls(walls)} /> Walls</label>
          <label><input type="checkbox" bind:checked={passage} onchange={() => view?.setShowPassage(passage)} /> Passage</label>
          <label title="Show the profile sketches along their runs (they fade out from the top or bottom, where they cannot be read)"><input type="checkbox" checked={cave.showSections.profileSketches} onchange={(e) => setShowSections('profileSketches', e.currentTarget.checked)} /> Profile sketches</label>
          <label title="Show the cross sections drawn at stations, next to their stations (drag one to place it)"><input type="checkbox" checked={cave.showSections.map} onchange={(e) => setShowSections('map', e.currentTarget.checked)} /> Sections</label>
          <label title="Draw a paler section through stations with no measurements"><input type="checkbox" bind:checked={fillGaps} onchange={loadPassage} /> Fill gaps</label>
          <h4>Types</h4>
          {#each listed as t}
            <label><input type="checkbox" checked={!filter.hiddenTypes.includes(t.key)} onchange={(e) => (setShown('types', t.key, e.currentTarget.checked), refresh())} /> {t.label}</label>
          {/each}
          {#if symbolTypes.length > 0}
            <h4>Symbols</h4>
            {#each symbolTypes as t}
              <label><input type="checkbox" checked={!filter.hiddenTypes.includes(t.key)} onchange={(e) => (setShown('types', t.key, e.currentTarget.checked), refresh())} /> {t.label}</label>
            {/each}
          {/if}
          {#if scene && scene.trips.length > 0}
            <h4>Trips</h4>
            {#each scene.trips as trip}
              <label><input type="checkbox" checked={!filter.hiddenTrips.includes(trip.id)} onchange={(e) => (setShown('trips', trip.id, e.currentTarget.checked), refresh())} /> {trip.name || 'Unnamed trip'}</label>
            {/each}
          {/if}
          {#if filter.hiddenTypes.length + filter.hiddenTrips.length > 0}
            <button onclick={() => ((filter.hiddenTypes = []), (filter.hiddenTrips = []), refresh())}>Show everything</button>
          {/if}
        </div>
      {/if}
    </div>
    <PanelUndo scope="map" label="on the map" />
    <button class:on={depthColor} title="Colour by elevation" onclick={() => ((depthColor = !depthColor), view?.setElevationColors(depthColor), loadPassage())}><Palette size={15} /></button>
    {#if drawHint}<span class="status">{drawHint}</span>{:else if status}<span class="status">{status}</span>{/if}
  </div>
  <div class="cube">
    <div bind:this={cubeHost}></div>
    <div class="cubebar">
      <button title="Home view" onclick={() => view?.home()}><Home size={15} /></button>
      <button title="Fit to cave" onclick={() => view?.fit()}><Maximize size={15} /></button>
      <button title={projection === 'perspective' ? 'Switch to orthographic' : 'Switch to perspective'} onclick={() => view?.setProjection(projection === 'perspective' ? 'orthographic' : 'perspective')}>
        {#if projection === 'perspective'}<Box size={15} />{:else}<Square size={15} />{/if}
      </button>
    </div>
  </div>
</div>

<style>
  .vp { outline: none; position: relative; height: 100%; overflow: hidden; background: var(--panel-2); }
  canvas { position: absolute; inset: 0; display: block; touch-action: none; }
  .overlay { pointer-events: none; }
  .bar { position: absolute; top: 8px; left: 8px; max-width: calc(100% - 160px); flex-wrap: wrap; display: flex; gap: 6px 10px; align-items: center; padding: 6px 8px; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); font-size: 12px; }
  .cube { position: absolute; top: 8px; right: 8px; display: flex; flex-direction: column; align-items: center; }
  .cubebar { display: flex; gap: 4px; margin-top: 2px; }
  select, button { font: inherit; padding: 3px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); color: var(--text); cursor: pointer; display: inline-grid; place-items: center; }
  button:disabled { opacity: 0.4; cursor: default; }
  button.on { background: var(--accent-bg); }
  label { display: flex; gap: 4px; align-items: center; color: var(--muted); white-space: nowrap; }
  .status { color: var(--muted); }
  .menu { position: relative; }
  .menu > button { gap: 5px; grid-auto-flow: column; }
  .pop { position: absolute; top: calc(100% + 10px); left: 0; z-index: 5; min-width: 200px; max-height: 60vh; overflow: auto; display: flex; flex-direction: column; gap: 4px; padding: 8px 10px; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); box-shadow: 0 6px 20px rgb(0 0 0 / 0.25); }
  .pop h4 { margin: 4px 0 0; font-size: 11px; text-transform: uppercase; letter-spacing: 0.05em; color: var(--muted); }
  .pop label { color: var(--text); }
</style>
