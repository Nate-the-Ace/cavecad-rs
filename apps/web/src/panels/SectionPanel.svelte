<script lang="ts">
  // Cross Section: the passage cut across at a station, drawn from the walls measured there (the
  // same ring the 3D tube is built from), with a pen or mouse to sketch the real shape on top.
  // Sections live at stations: pick one here (or click a station on the map). Sketches belong to
  // their station and follow it when the survey is adjusted.
  import { onMount, untrack } from 'svelte';
  import { onTheme } from '../lib/theme';
  import { Undo2, Redo2, Maximize, ChevronLeft, ChevronRight } from '@lucide/svelte';
  import { looks } from '../lib/ink';
  import { drawStrokes } from '../lib/section-glyph';
  import { cave, changed, filter, wireFilter } from '../lib/cave.svelte';
  import { engine, loadEngine, type SectionStation, type StationSection } from '../lib/engine';
  import { metresPer, niceNumber, unitName } from '../lib/units';

  let wrap: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let width = $state(0);
  let height = $state(0);
  let ready = $state(false);
  let stations = $state.raw<SectionStation[]>([]);
  let section = $state.raw<StationSection | null>(null);
  let canUndo = $state(false);
  let canRedo = $state(false);
  let message = $state('');
  let barH = $state(50);

  // View: pixels per metre, and where the station (the origin) sits on the canvas.
  let scale = 20;
  let ox = 0;
  let oy = 0;
  let frame = 0;
  let framed = '';
  let live: { pts: [number, number][]; widths: number[]; pressure: boolean } | null = null;

  const index = $derived(stations.findIndex((s) => s.id === cave.station));
  const toWorld = (x: number, y: number): [number, number] => [(x - ox) / scale, (oy - y) / scale];
  const toScreen = (u: number, v: number): [number, number] => [ox + u * scale, oy - v * scale];

  function refresh() {
    if (!ready) return;
    stations = engine.sectionStations(cave.adjusted, true, wireFilter());
    if (!stations.some((s) => s.id === cave.station)) cave.station = (stations.find((s) => s.outlined) ?? stations[0])?.id ?? '';
    section = cave.station ? engine.sectionAt(cave.adjusted, cave.station, true, wireFilter()) : null;
    const st = engine.editState('section');
    canUndo = st.undo;
    canRedo = st.redo;
    frameOnce();
    schedule();
  }

  // Re-read whenever the cave changes, a station is picked, or the adjustment or trip filter does.
  $effect(() => {
    void cave.version;
    void cave.adjusted;
    void cave.station;
    void filter.hiddenTrips.length;
    void ready;
    untrack(refresh);
  });

  $effect(() => {
    void cave.units;
    void barH;
    void width;
    void height;
    untrack(frameOnce);
    schedule();
  });

  /** Frames the section whenever a different station is shown (not after every stroke). */
  function frameOnce() {
    if (section && framed !== section.id && width > 0) {
      framed = section.id;
      fit();
    }
  }

  function fit() {
    const s = section;
    if (!s || width === 0) return;
    const all = [...s.outline, ...s.measured, ...s.sketches.flatMap((k) => k.points)];
    const reach = Math.max(1, ...all.flatMap((p) => [Math.abs(p[0]), Math.abs(p[1])])) * 1.12;
    const top = barH + 16;
    const room = [width / 2 - 20, (height - top - 48) / 2];
    scale = Math.max(2, Math.min(room[0], room[1]) / reach);
    ox = width / 2;
    oy = top + (height - top - 48) / 2;
    schedule();
  }

  function schedule() {
    if (!frame) frame = requestAnimationFrame(() => ((frame = 0), draw()));
  }

  const css = () => {
    const c = getComputedStyle(document.documentElement);
    const v = (n: string) => c.getPropertyValue(n).trim();
    return { text: v('--text'), muted: v('--muted'), line: v('--line'), accent: v('--accent'), accentBg: v('--accent-bg') };
  };

  function draw() {
    if (!canvas || width === 0) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    const g = canvas.getContext('2d')!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, width, height);
    const c = css();
    const s = section;
    if (!s) return;

    // axis cross at the station
    const [cx, cy] = toScreen(0, 0);
    g.strokeStyle = c.line;
    g.lineWidth = 1;
    g.beginPath();
    g.moveTo(cx - 14, cy);
    g.lineTo(cx + 14, cy);
    g.moveTo(cx, cy - 14);
    g.lineTo(cx, cy + 14);
    g.stroke();

    // the measured outline: a guide to trace against, never the passage itself
    if (s.outline.length >= 3) {
      g.beginPath();
      s.outline.forEach((p, i) => {
        const [x, y] = toScreen(p[0], p[1]);
        if (i === 0) g.moveTo(x, y);
        else g.lineTo(x, y);
      });
      g.closePath();
      g.fillStyle = c.accentBg;
      g.globalAlpha = 0.4;
      g.fill();
      g.globalAlpha = 1;
      g.setLineDash([6, 4]);
      g.strokeStyle = c.muted;
      g.lineWidth = 1.5;
      g.lineJoin = 'round';
      g.stroke();
      g.setLineDash([]);
    }
    g.fillStyle = c.muted;
    for (const p of s.measured) {
      const [x, y] = toScreen(p[0], p[1]);
      g.beginPath();
      g.arc(x, y, 3.2, 0, Math.PI * 2);
      g.fill();
    }

    // what has been sketched
    const look = looks();
    drawStrokes(g, s.sketches, toScreen, look);
    if (live && live.pts.length > 1) drawStrokes(g, [{ kind: cave.tool || 'sketch', points: live.pts, widths: live.pressure ? live.widths : null }], toScreen, look);

    // up arrow and what it means
    g.fillStyle = c.muted;
    g.strokeStyle = c.muted;
    g.font = '11px system-ui, sans-serif';
    g.lineWidth = 1.5;
    const ay = height - 56;
    g.beginPath();
    g.moveTo(width - 28, ay);
    g.lineTo(width - 28, ay - 26);
    g.moveTo(width - 33, ay - 20);
    g.lineTo(width - 28, ay - 26);
    g.lineTo(width - 23, ay - 20);
    g.stroke();
    g.textAlign = 'right';
    g.fillText(s.up_means === 'north' ? 'north' : 'up', width - 38, ay - 10);

    // scale bar in the chosen units
    const per = metresPer(cave.units);
    const len = niceNumber(100 / scale / per);
    g.strokeStyle = c.text;
    g.fillStyle = c.text;
    g.lineWidth = 2;
    g.textAlign = 'left';
    g.beginPath();
    g.moveTo(16, height - 44);
    g.lineTo(16 + len * per * scale, height - 44);
    g.stroke();
    g.fillText(`${len} ${unitName(cave.units)}`, 16, height - 50);
  }

  // ---- input: pen and mouse draw, fingers and middle button pan, wheel zooms -------------------
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
    const draws = !!cave.tool && !middle && e.button === 0 && e.pointerType !== 'touch' && !!section;
    if (draws) {
      live = { pts: [], widths: [], pressure: e.pointerType === 'pen' };
      addPoint(e);
    } else {
      panning = { x: e.clientX, y: e.clientY };
    }
  }

  function onMove(e: PointerEvent) {
    if (live) {
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
    if (stroked.pts.length >= 2 && cave.station) {
      const result = engine.addSectionSketch(cave.station, cave.tool, stroked.pts, stroked.pressure ? stroked.widths : null, cave.adjusted);
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
    if (engine.undo('section')) changed();
  }
  function doRedo() {
    if (engine.redo('section')) changed();
  }
  function onKey(e: KeyboardEvent) {
    if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== 'z') return;
    e.preventDefault();
    if (e.shiftKey) doRedo();
    else doUndo();
  }

  function step(by: number) {
    const next = stations[index + by];
    if (next) cave.station = next.id;
  }

  const caption = $derived.by(() => {
    const s = section;
    if (!s) return '';
    if (s.note) return s.note;
    const n = s.measured.length;
    const base = `${n} measured wall point${n === 1 ? '' : 's'} at ${s.label}${s.inferred ? ', inferred from neighbouring stations (no measurements here)' : ''}. The dashed outline is a guide to trace against.`;
    return base;
  });

  onMount(() => {
    const ro = new ResizeObserver(() => {
      width = wrap.clientWidth;
      height = wrap.clientHeight;
      schedule();
    });
    ro.observe(wrap);
    const offTheme = onTheme(schedule);
    loadEngine().then(() => (ready = true));
    return () => {
      ro.disconnect();
      offTheme();
      cancelAnimationFrame(frame);
    };
  });
</script>

<div class="section" bind:this={wrap}>
  <canvas
    bind:this={canvas}
    tabindex="0"
    style="width:{width}px;height:{height}px;cursor:{cave.tool ? 'crosshair' : 'grab'}"
    onpointerdown={onDown}
    onpointermove={onMove}
    onpointerup={onUp}
    onpointercancel={onUp}
    onwheel={onWheel}
    onkeydown={onKey}
    oncontextmenu={(e) => e.preventDefault()}
  ></canvas>
  <div class="bar" bind:clientHeight={barH}>
    <button title="Previous station" disabled={index <= 0} onclick={() => step(-1)}><ChevronLeft size={15} /></button>
    <select bind:value={cave.station} aria-label="Station">
      {#each stations as s}<option value={s.id}>{s.label}{s.outlined ? '' : ' (no walls)'}{s.sketches ? ` · ${s.sketches} sketch${s.sketches === 1 ? '' : 'es'}` : ''}</option>{/each}
    </select>
    <button title="Next station" disabled={index < 0 || index >= stations.length - 1} onclick={() => step(1)}><ChevronRight size={15} /></button>
    <button title="Undo (Ctrl/Cmd+Z)" disabled={!canUndo} onclick={doUndo}><Undo2 size={15} /></button>
    <button title="Redo (Shift+Ctrl/Cmd+Z)" disabled={!canRedo} onclick={doRedo}><Redo2 size={15} /></button>
    <button title="Fit (or double middle-click)" onclick={fit}><Maximize size={15} /></button>
  </div>
  {#if ready && !stations.length}
    <div class="empty">Open a cave with surveyed legs to cut cross sections at its stations.</div>
  {:else if message}
    <div class="note warn" role="status">{message}</div>
  {:else if caption}
    <div class="note" class:warn={!!section?.note} role="status">{caption}</div>
  {/if}
</div>

<style>
  .section { position: relative; height: 100%; overflow: hidden; background: var(--panel-2); }
  canvas { position: absolute; inset: 0; display: block; touch-action: none; outline: none; }
  .bar { position: absolute; top: 8px; left: 8px; right: 8px; display: flex; gap: 6px; align-items: center; flex-wrap: wrap; padding: 6px 8px; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); font-size: 12px; width: fit-content; max-width: calc(100% - 16px); }
  select, button { font: inherit; padding: 3px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); color: var(--text); cursor: pointer; display: inline-grid; place-items: center; }
  select { max-width: 200px; }
  button:disabled { opacity: 0.4; cursor: default; }
  .note { position: absolute; left: 8px; right: 8px; bottom: 8px; padding: 6px 10px; font-size: 12px; color: var(--muted); background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); }
  .note.warn { color: var(--text); border-color: var(--warn); }
  .empty { position: absolute; inset: 0; display: grid; place-items: center; padding: 16px; text-align: center; color: var(--muted); font-size: 13px; }
</style>
