<script lang="ts">
  import { onMount } from 'svelte';
  import { Home, Maximize, Box, Square, Palette } from '@lucide/svelte';
  import { DEMOS } from '../lib/demos';
  import { engine, loadEngine, type Scene } from '../lib/engine';
  import { CaveView, type Projection } from '../lib/viewport/cave-view';
  import { cave, changed } from '../lib/cave.svelte';
  import { ViewCube } from '../lib/viewport/view-cube';

  let wrap: HTMLDivElement;
  let gl: HTMLCanvasElement;
  let overlay: HTMLCanvasElement;
  let cubeHost: HTMLDivElement;
  let view: CaveView | null = null;

  let demo = $state(DEMOS[0].id);
  let depthColor = $state(false);
  let walls = $state(true);
  let passage = $state(true);
  let fillGaps = $state(true);
  let projection = $state<Projection>('perspective');
  let status = $state('');
  let scene: Scene | null = null;

  function open(id: string) {
    const d = DEMOS.find((x) => x.id === id);
    if (!d || !view) return;
    status = engine.openDemo(d.json);
    cave.run = '';
    changed();
    scene = engine.scene(cave.adjusted);
    view.setScene(scene);
    loadPassage();
  }

  /** The passage surface follows the Adjusted and colour settings. */
  function loadPassage() {
    const buffers = engine.passageMesh(cave.adjusted, depthColor ? 'depth' : 'trip', fillGaps);
    view?.setPassage(buffers);
    if (buffers.filled > 0) {
      engine.log('info', 'passage', `${buffers.filled} unmeasured station${buffers.filled === 1 ? '' : 's'} filled in from the nearest measurements (shown paler)`);
    }
  }

  function refresh() {
    if (!view) return;
    scene = engine.scene(cave.adjusted);
    view.setScene(scene, false);
    loadPassage();
    changed();
  }

  onMount(() => {
    const v = new CaveView(gl, overlay);
    const cube = new ViewCube(cubeHost, (dir) => v.snapTo(dir), (da, dp) => v.orbitBy(da, dp));
    v.onRender = (camera, target) => cube.sync(camera, target);
    v.onProjection = (p) => (projection = p);
    v.shadingFor = (dir, persp) => engine.shading(cave.adjusted, dir, persp);
    view = v;
    if (import.meta.env.DEV) (window as unknown as { __view: CaveView }).__view = v; // for tests

    const ro = new ResizeObserver(() => v.resize(wrap.clientWidth, wrap.clientHeight));
    ro.observe(wrap);
    const dark = window.matchMedia('(prefers-color-scheme: dark)');
    const retheme = () => v.applyTheme();
    dark.addEventListener('change', retheme);
    loadEngine().then(() => open(demo));

    return () => {
      ro.disconnect();
      dark.removeEventListener('change', retheme);
      cube.dispose();
      v.dispose();
      view = null;
    };
  });
</script>

<div class="vp" bind:this={wrap}>
  <canvas bind:this={gl}></canvas>
  <canvas bind:this={overlay} class="overlay"></canvas>
  <div class="bar">
    <select bind:value={demo} onchange={() => open(demo)} aria-label="Demo cave">
      {#each DEMOS as d}<option value={d.id}>{d.title}</option>{/each}
    </select>
    <label><input type="checkbox" bind:checked={cave.adjusted} onchange={refresh} /> Adjusted</label>
    <label><input type="checkbox" bind:checked={walls} onchange={() => view?.setShowWalls(walls)} /> Walls</label>
    <label><input type="checkbox" bind:checked={passage} onchange={() => view?.setShowPassage(passage)} /> Passage</label>
    <label title="Draw a paler section through stations with no measurements"><input type="checkbox" bind:checked={fillGaps} onchange={loadPassage} /> Fill gaps</label>
    <button class:on={depthColor} title="Colour by elevation" onclick={() => ((depthColor = !depthColor), view?.setElevationColors(depthColor), loadPassage())}><Palette size={15} /></button>
    {#if status}<span class="status">{status}</span>{/if}
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
  .vp { position: relative; height: 100%; overflow: hidden; background: var(--panel-2); }
  canvas { position: absolute; inset: 0; display: block; touch-action: none; }
  .overlay { pointer-events: none; }
  .bar { position: absolute; top: 8px; left: 8px; display: flex; gap: 8px; align-items: center; padding: 6px 8px; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); font-size: 12px; }
  .cube { position: absolute; top: 8px; right: 8px; display: flex; flex-direction: column; align-items: center; }
  .cubebar { display: flex; gap: 4px; margin-top: 2px; }
  select, button { font: inherit; padding: 3px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); color: var(--text); cursor: pointer; display: inline-grid; place-items: center; }
  button.on { background: var(--accent-bg); }
  label { display: flex; gap: 4px; align-items: center; color: var(--muted); }
  .status { color: var(--muted); }
</style>
