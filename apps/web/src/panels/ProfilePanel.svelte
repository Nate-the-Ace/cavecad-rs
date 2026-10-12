<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { Pencil, Hand, Undo2, Redo2, Maximize } from '@lucide/svelte';
  import { cave, changed } from '../lib/cave.svelte';
  import { engine, loadEngine, type ProfileScene, type RunInfo } from '../lib/engine';

  let wrap: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let width = $state(0);
  let height = $state(0);
  let runs = $state<RunInfo[]>([]);
  let band = $state<ProfileScene | null>(null);
  let tool = $state<'sketch' | 'pan'>('sketch');
  let canUndo = $state(false);
  let canRedo = $state(false);
  let message = $state('');

  // View: pixels per metre (uniform, so the profile is true to scale), and where s=0, z=0 sits.
  let scale = 1;
  let ox = 0;
  let oy = 0;
  let frame = 0;
  let live: { pts: [number, number][]; widths: number[]; pressure: boolean } | null = null;

  const toWorld = (x: number, y: number): [number, number] => [(x - ox) / scale, (oy - y) / scale];
  const toScreen = (s: number, z: number): [number, number] => [ox + s * scale, oy - z * scale];

  let ready = false;

  function refresh() {
    if (!wrap || !ready) return;
    runs = engine.profileRuns(cave.adjusted);
    if (!runs.some((r) => r.key === cave.run)) cave.run = runs[0]?.key ?? '';
    band = cave.run ? engine.profileBand(cave.run, cave.adjusted) : null;
    const state = engine.editState();
    canUndo = state.undo;
    canRedo = state.redo;
    schedule();
  }

  // Re-read whenever a cave opens, is edited, or the run or adjustment changes.
  $effect(() => {
    void cave.version;
    void cave.adjusted;
    void cave.run;
    // refresh() both reads and writes panel state; only the three values above may re-trigger it.
    untrack(refresh);
  });
  // Frame the band only when the run itself changes, not after every sketch.
  let framed = '';
  $effect(() => {
    const key = band ? `${band.key}:${band.stations.length}` : '';
    const w = width;
    untrack(() => {
      if (key !== framed && w > 0) {
        framed = key;
        fit();
      }
    });
  });

  function fit() {
    if (!band?.bounds || width === 0) return;
    const [s0, z0, s1, z1] = band.bounds;
    const w = Math.max(s1 - s0, 1);
    const h = Math.max(z1 - z0, 1);
    const pad = 48;
    scale = Math.min((width - 2 * pad) / w, (height - 2 * pad) / h);
    ox = (width - scale * (s0 + s1)) / 2;
    oy = (height + scale * (z0 + z1)) / 2;
    schedule();
  }

  function schedule() {
    if (!frame) frame = requestAnimationFrame(() => ((frame = 0), draw()));
  }

  const css = () => {
    const c = getComputedStyle(document.documentElement);
    const v = (n: string) => c.getPropertyValue(n).trim();
    return { bg: v('--panel-2'), text: v('--text'), muted: v('--muted'), accent: v('--accent'), line: v('--line'), warn: v('--warn') };
  };

  function niceStep(target: number): number {
    const p = Math.pow(10, Math.floor(Math.log10(target)));
    const m = target / p;
    return (m >= 5 ? 5 : m >= 2 ? 2 : 1) * p;
  }

  function stroke(g: CanvasRenderingContext2D, pts: [number, number][], widths: number[] | null, color: string, base = 2.6) {
    g.strokeStyle = color;
    g.lineCap = 'round';
    g.lineJoin = 'round';
    for (let i = 1; i < pts.length; i++) {
      const a = toScreen(pts[i - 1][0], pts[i - 1][1]);
      const b = toScreen(pts[i][0], pts[i][1]);
      g.lineWidth = base * (widths ? (widths[i - 1] + widths[i]) / 2 : 1);
      g.beginPath();
      g.moveTo(a[0], a[1]);
      g.lineTo(b[0], b[1]);
      g.stroke();
    }
  }

  function draw() {
    if (!canvas || width === 0) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    const g = canvas.getContext('2d')!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const c = css();
    g.fillStyle = c.bg;
    g.fillRect(0, 0, width, height);
    g.font = '11px system-ui, sans-serif';
    if (!band) {
      g.fillStyle = c.muted;
      g.textAlign = 'center';
      g.fillText('Open a cave, then pick a run.', width / 2, height / 2);
      return;
    }

    // grid: elevation lines every nice step, labelled at the left
    const step = niceStep(60 / scale);
    g.strokeStyle = c.line;
    g.fillStyle = c.muted;
    g.lineWidth = 1;
    g.textAlign = 'left';
    const [zTop] = toWorld(0, 0).slice(1);
    const [zBottom] = toWorld(0, height).slice(1);
    for (let z = Math.ceil(zBottom / step) * step; z <= zTop; z += step) {
      const y = toScreen(0, z)[1];
      g.beginPath();
      g.moveTo(0, y);
      g.lineTo(width, y);
      g.stroke();
      g.fillText(`${+z.toFixed(2)} m`, 6, y - 3);
    }

    g.setLineDash([5, 4]);
    for (const line of [...band.ceiling, ...band.floor]) stroke(g, line, null, c.muted, 1.2);
    g.setLineDash([]);
    for (const l of band.legs) stroke(g, [[l[0], l[1]], [l[2], l[3]]], null, c.text, 2);
    for (const k of band.sketches) stroke(g, k.points, k.widths, c.accent);
    if (live && live.pts.length > 1) stroke(g, live.pts, live.pressure ? live.widths : null, c.accent);

    g.fillStyle = c.text;
    g.textAlign = 'left';
    for (const st of band.stations) {
      const [x, y] = toScreen(st.s, st.z);
      g.beginPath();
      g.arc(x, y, 3, 0, Math.PI * 2);
      g.fill();
      g.fillStyle = c.muted;
      g.fillText(st.label, x + 5, y - 6);
      g.fillStyle = c.text;
    }
  }

  // ---- input: pen and mouse draw, fingers pan, middle button pans, wheel zooms ---------------
  let panning: { x: number; y: number } | null = null;
  let lastMiddle = 0;

  function addPoint(e: PointerEvent) {
    if (!live) return;
    const rect = canvas.getBoundingClientRect();
    const p = toWorld(e.clientX - rect.left, e.clientY - rect.top);
    const last = live.pts[live.pts.length - 1];
    if (last && Math.hypot(p[0] - last[0], p[1] - last[1]) * scale < 1.5) return;
    live.pts.push(p);
    // Pressure only changes how thick the line is drawn, never where it goes.
    live.widths.push(e.pointerType === 'pen' ? 0.4 + 1.2 * e.pressure : 1);
  }

  function onDown(e: PointerEvent) {
    canvas.setPointerCapture(e.pointerId);
    canvas.focus();
    const middle = e.button === 1;
    if (middle) {
      e.preventDefault();
      if (e.timeStamp - lastMiddle < 350) fit();
      lastMiddle = e.timeStamp;
    }
    const draws = tool === 'sketch' && !middle && e.button === 0 && e.pointerType !== 'touch' && !!band;
    if (draws) {
      live = { pts: [], widths: [], pressure: e.pointerType === 'pen' };
      addPoint(e);
    } else {
      panning = { x: e.clientX, y: e.clientY };
    }
  }

  function onMove(e: PointerEvent) {
    if (live) {
      // Coalesced events give every sample the pen produced between frames. Some browsers return
      // none (or no such method), so fall back to the event itself.
      const samples = e.getCoalescedEvents?.() ?? [];
      for (const ev of samples.length > 0 ? samples : [e]) addPoint(ev);
      schedule();
    } else if (panning) {
      ox += e.clientX - panning.x;
      oy += e.clientY - panning.y;
      panning = { x: e.clientX, y: e.clientY };
      schedule();
    }
  }

  function onUp() {
    panning = null;
    if (!live) return;
    const stroked = live;
    live = null;
    if (stroked.pts.length >= 2 && cave.run) {
      const result = engine.addSketch(cave.run, stroked.pts, stroked.pressure ? stroked.widths : null, cave.adjusted);
      message = result === 'ok' ? '' : result;
      changed();
    }
    schedule();
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const f = Math.exp(-e.deltaY * 0.0015);
    ox = mx - (mx - ox) * f;
    oy = my - (my - oy) * f;
    scale *= f;
    schedule();
  }

  function doUndo() {
    if (engine.undo()) changed();
  }
  function doRedo() {
    if (engine.redo()) changed();
  }
  function onKey(e: KeyboardEvent) {
    if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== 'z') return;
    e.preventDefault();
    if (e.shiftKey) doRedo();
    else doUndo();
  }

  onMount(() => {
    const ro = new ResizeObserver(() => {
      width = wrap.clientWidth;
      height = wrap.clientHeight;
      schedule();
    });
    ro.observe(wrap);
    const dark = window.matchMedia('(prefers-color-scheme: dark)');
    dark.addEventListener('change', schedule);
    loadEngine().then(() => {
      ready = true;
      refresh();
    });
    return () => {
      ro.disconnect();
      dark.removeEventListener('change', schedule);
    };
  });
</script>

<div class="profile" bind:this={wrap}>
  <canvas
    bind:this={canvas}
    tabindex="0"
    style="width:{width}px;height:{height}px;cursor:{tool === 'sketch' ? 'crosshair' : 'grab'}"
    onpointerdown={onDown}
    onpointermove={onMove}
    onpointerup={onUp}
    onpointercancel={onUp}
    onwheel={onWheel}
    onkeydown={onKey}
    oncontextmenu={(e) => e.preventDefault()}
  ></canvas>
  <div class="bar">
    <select bind:value={cave.run} aria-label="Run">
      {#each runs as r}<option value={r.key}>{r.key} · {r.stations} stations · {Math.round(r.length)} m{r.tie ? ` · from ${r.tie}` : ''}</option>{/each}
    </select>
    <button class:on={tool === 'sketch'} title="Sketch (pen or mouse)" onclick={() => (tool = 'sketch')}><Pencil size={15} /></button>
    <button class:on={tool === 'pan'} title="Pan" onclick={() => (tool = 'pan')}><Hand size={15} /></button>
    <button title="Undo (Ctrl/Cmd+Z)" disabled={!canUndo} onclick={doUndo}><Undo2 size={15} /></button>
    <button title="Redo (Shift+Ctrl/Cmd+Z)" disabled={!canRedo} onclick={doRedo}><Redo2 size={15} /></button>
    <button title="Fit band (or double middle-click)" onclick={fit}><Maximize size={15} /></button>
  </div>
  {#if band?.note || message || (band && (band.omitted > 0 || band.unplaced > 0))}
    <div class="note">
      {#if message}{message}{:else if band?.note}{band.note}{:else if band && band.unplaced > 0}{band.unplaced} sketch{band.unplaced === 1 ? '' : 'es'} hang on stations that are not on this band.{:else if band}{band.omitted} station{band.omitted === 1 ? '' : 's'} on side branches are not on this band.{/if}
    </div>
  {/if}
</div>

<style>
  .profile { position: relative; height: 100%; overflow: hidden; background: var(--panel-2); }
  canvas { position: absolute; inset: 0; display: block; touch-action: none; outline: none; }
  .bar { position: absolute; top: 8px; left: 8px; right: 8px; display: flex; gap: 6px; align-items: center; padding: 6px 8px; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); font-size: 12px; width: fit-content; max-width: calc(100% - 16px); }
  select, button { font: inherit; padding: 3px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); color: var(--text); cursor: pointer; display: inline-grid; place-items: center; }
  select { max-width: 260px; }
  button.on { background: var(--accent-bg); }
  button:disabled { opacity: 0.4; cursor: default; }
  .note { position: absolute; left: 8px; bottom: 8px; padding: 4px 8px; font-size: 12px; color: var(--muted); background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); }
</style>
