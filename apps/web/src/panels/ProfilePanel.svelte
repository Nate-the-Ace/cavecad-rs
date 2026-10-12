<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { onTheme } from '../lib/theme';
  import { Maximize } from '@lucide/svelte';
  import PanelUndo from '../components/PanelUndo.svelte';
  import { looks } from '../lib/ink';
  import { cave, changed, option, setShowSections } from '../lib/cave.svelte';
  import { defaultBandOffset, drawHandles, drawStrokes, glyphCurves, hits, leaderEnd } from '../lib/section-glyph';
  import { LineEditor } from '../lib/line-editor';
  import { simplify } from '../lib/spline';
  import { tolerancePx } from '../lib/commands';
  import type { BandSection } from '../lib/engine';
  import { formatLength, inUnits, metresPer, niceNumber, unitName } from '../lib/units';
  import { engine, loadEngine, type ProfileScene, type RunInfo } from '../lib/engine';

  let wrap: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let width = $state(0);
  let height = $state(0);
  let runs = $state<RunInfo[]>([]);
  let band = $state<ProfileScene | null>(null);
  let message = $state('');

  // View: pixels per metre (uniform, so the profile is true to scale), and where s=0, z=0 sits.
  let scale = 1;
  let ox = 0;
  let oy = 0;
  let frame = 0;
  // Drawn lines can be selected and their fit points moved, added and deleted.
  const editor = new LineEditor({
    lines: () => band?.sketches ?? [],
    toScreen: (p) => toScreen(p[0], p[1]),
    toWorld: (x, y) => toWorld(x, y),
    selected: () => cave.selected,
    select: (id) => (cave.selected = id),
    commit: (id, pts, widths) => {
      const r = engine.editLine(id, pts, widths, cave.adjusted);
      message = r === 'ok' ? '' : r;
      changed();
    },
    remove: (id) => {
      engine.deleteLine(id);
      changed();
    },
    redraw: () => schedule(),
  });
  $effect(() => {
    void cave.selected;
    schedule();
  });

  // Cross-section sketches sit beside their stations; a drag moves one, and the drop is saved.
  const liveOffsets = new Map<string, [number, number]>();
  let dragSection: { id: string; x: number; y: number } | null = null;
  const offsetOf = (s: BandSection): [number, number] => liveOffsets.get(s.id) ?? s.placement ?? defaultBandOffset(s.sketches);
  const anchorOf = (s: BandSection): [number, number] => {
    const o = offsetOf(s);
    return toScreen(s.s + o[0], s.z + o[1]);
  };
  const sectionAt = (x: number, y: number): BandSection | null => {
    if (!cave.showSections.profile || !band) return null;
    for (const s of [...band.sections].reverse()) if (hits(s.sketches, anchorOf(s), scale, x, y)) return s;
    return null;
  };
  let live: { pts: [number, number][]; widths: number[]; pressure: boolean } | null = null;

  const toWorld = (x: number, y: number): [number, number] => [(x - ox) / scale, (oy - y) / scale];
  const toScreen = (s: number, z: number): [number, number] => [ox + s * scale, oy - z * scale];

  let ready = false;

  function refresh() {
    if (!wrap || !ready) return;
    runs = engine.profileRuns(cave.adjusted);
    if (!runs.some((r) => r.key === cave.run)) cave.run = runs[0]?.key ?? '';
    band = cave.run ? engine.profileBand(cave.run, cave.adjusted) : null;
    schedule();
  }

  // Re-read whenever a cave opens, is edited, or the run or adjustment changes.
  $effect(() => {
    void cave.version;
    void cave.adjusted;
    void cave.run;
    void cave.showSections.profile; // sketches beside stations appear and disappear with the toggle
    void cave.units; // axis labels and run lengths redraw in the new units
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
    const per = metresPer(cave.units); // metres in one display unit
    const step = niceNumber(60 / scale / per) * per;
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
      g.fillText(`${+inUnits(z, cave.units).toFixed(2)} ${unitName(cave.units)}`, 6, y - 3);
    }

    g.setLineDash([5, 4]);
    for (const line of [...band.ceiling, ...band.floor]) stroke(g, line, null, c.muted, 1.2);
    g.setLineDash([]);
    for (const l of band.legs) stroke(g, [[l[0], l[1]], [l[2], l[3]]], null, c.text, 2);
    const look = looks();
    drawStrokes(g, editor.display(), toScreen, look, 1, cave.selected);
    drawHandles(g, editor.handles(), c.accent, c.bg);
    if (cave.showSections.profile) {
      for (const sec of band.sections) {
        const st = toScreen(sec.s, sec.z);
        const at = anchorOf(sec);
        const to = (u: number, v: number): [number, number] => [at[0] + u * scale, at[1] - v * scale];
        const shell = leaderEnd(st, glyphCurves(sec.sketches, to));
        // The leader ties the drawing to its station and stops at the drawing's outer shell.
        g.strokeStyle = c.muted;
        g.lineWidth = 1;
        g.setLineDash([4, 3]);
        g.beginPath();
        g.moveTo(st[0], st[1]);
        g.lineTo(shell.end[0], shell.end[1]);
        g.stroke();
        g.setLineDash([]);
        drawStrokes(g, sec.sketches, to, look);
        g.fillStyle = c.muted;
        g.textAlign = 'center';
        g.fillText(sec.label, shell.below[0], shell.below[1]);
        g.textAlign = 'left';
      }
    }
    if (live && live.pts.length > 1) drawStrokes(g, [{ kind: cave.tool || 'sketch', points: live.pts, widths: live.pressure ? live.widths : null }], toScreen, look);

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
    live.widths.push(live.pressure ? 0.4 + 1.2 * e.pressure : 1);
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
    if (e.button === 0 && e.pointerType !== 'touch' && !cave.tool) {
      const r = canvas.getBoundingClientRect();
      if (editor.press(e.clientX - r.left, e.clientY - r.top)) return;
    }
    if (e.button === 0 && e.pointerType !== 'touch') {
      const rect = canvas.getBoundingClientRect();
      const grab = sectionAt(e.clientX - rect.left, e.clientY - rect.top);
      if (grab) {
        dragSection = { id: grab.id, x: e.clientX, y: e.clientY };
        return;
      }
    }
    const draws = !!cave.tool && !middle && e.button === 0 && e.pointerType !== 'touch' && !!band;
    if (draws) {
      live = { pts: [], widths: [], pressure: e.pointerType === 'pen' && option('pressure', true) };
      addPoint(e);
    } else {
      panning = { x: e.clientX, y: e.clientY };
    }
  }

  function onMove(e: PointerEvent) {
    const r = canvas.getBoundingClientRect();
    if (editor.move(e.clientX - r.left, e.clientY - r.top)) return;
    if (dragSection) {
      const s = band?.sections.find((k) => k.id === dragSection!.id);
      if (s) {
        const o = offsetOf(s);
        liveOffsets.set(s.id, [o[0] + (e.clientX - dragSection.x) / scale, o[1] - (e.clientY - dragSection.y) / scale]);
      }
      dragSection.x = e.clientX;
      dragSection.y = e.clientY;
      schedule();
    } else if (live) {
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
    if (editor.release()) return;
    if (dragSection) {
      const id = dragSection.id;
      dragSection = null;
      const o = liveOffsets.get(id);
      if (o) {
        const r = engine.placeSection(id, 'profile', [o[0], o[1], 0]);
        if (r !== 'ok') message = r;
        changed();
      }
      liveOffsets.clear();
      return;
    }
    if (!live) return;
    const stroked = live;
    live = null;
    if (stroked.pts.length >= 2 && cave.run) {
      // A freehand stroke becomes a smooth curve through a few fit points.
      const fit = simplify(stroked.pts, stroked.pressure ? stroked.widths : null, tolerancePx(option('detail', 5)) / scale);
      const result = engine.addSketch(cave.run, cave.tool, fit.pts, fit.widths, cave.adjusted);
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
    if (engine.undo('profile')) changed();
  }
  function doRedo() {
    if (engine.redo('profile')) changed();
  }
  function onKey(e: KeyboardEvent) {
    if (editor.key(e)) {
      e.preventDefault();
      return;
    }
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
    const offTheme = onTheme(schedule);
    loadEngine().then(() => {
      ready = true;
      refresh();
    });
    return () => {
      ro.disconnect();
      offTheme();
    };
  });
</script>

<div class="profile" bind:this={wrap}>
  <canvas
    bind:this={canvas}
    tabindex="0"
    style="width:{width}px;height:{height}px;cursor:{cave.tool ? 'crosshair' : 'grab'}"
    onpointerdown={onDown}
    onpointermove={onMove}
    onpointerup={onUp}
    onpointercancel={onUp}
    onwheel={onWheel}
    ondblclick={(e) => {
      const r = canvas.getBoundingClientRect();
      if (!cave.tool) editor.addPoint(e.clientX - r.left, e.clientY - r.top);
    }}
    onkeydown={onKey}
    oncontextmenu={(e) => e.preventDefault()}
  ></canvas>
  <div class="bar">
    <select bind:value={cave.run} aria-label="Run">
      {#each runs as r}<option value={r.key}>{r.key} · {r.stations} stations · {formatLength(r.length, cave.units)}{r.tie ? ` · from ${r.tie}` : ''}</option>{/each}
    </select>
    <label class="show" title="Show the cross sections drawn at stations, next to their stations (drag one to place it)"><input type="checkbox" checked={cave.showSections.profile} onchange={(e) => (setShowSections('profile', e.currentTarget.checked), schedule())} /> Sections</label>
    <PanelUndo scope="profile" label="in the profile" />
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
  .show { display: flex; gap: 4px; align-items: center; color: var(--muted); }
  button:disabled { opacity: 0.4; cursor: default; }
  .note { position: absolute; left: 8px; bottom: 8px; padding: 4px 8px; font-size: 12px; color: var(--muted); background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); }
</style>
