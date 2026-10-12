<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { onTheme } from '../lib/theme';
  import { Maximize } from '@lucide/svelte';
  import PanelUndo from '../components/PanelUndo.svelte';
  import { looks } from '../lib/ink';
  import { cave, changed, filter, option, setShowSections } from '../lib/cave.svelte';
  import { defaultBandOffset, drawHandles, drawStrokes, glyphCurves, hits, leaderEnd } from '../lib/section-glyph';
  import { LineEditor, endpointNear, joinFit } from '../lib/line-editor';
  import { Touches } from '../lib/touch';
  import { PenGuard, isEraser, widthFor } from '../lib/pen';
  import { isStampKind } from '../lib/symbols';
  import { isFillKind } from '../lib/fills';
  import { cursorsFor, live as room, reportCursor } from '../lib/sync.svelte';
  import { drawCursor } from '../lib/presence-draw';
  import { fitStroke } from '../lib/spline';
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
    hidden: (kind) => !!kind && filter.hiddenTypes.includes(kind),
  });
  $effect(() => {
    void cave.selected;
    schedule();
  });
  // the selected line is fitted again at another detail
  let handledRefit = cave.refit.seq; // a panel opened later does not redo old requests
  $effect(() => {
    const r = cave.refit;
    if (r.seq === handledRefit) return;
    handledRefit = r.seq;
    untrack(() => editor.refitLine(r.id, r.detail));
  });
  // other people's pointers move
  $effect(() => {
    for (const o of Object.values(room.cursors)) void o.seen;
    void room.people.length;
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
    // near-level splays: real wall hits that say nothing about the ceiling or floor, so ticks, not line points
    g.fillStyle = c.muted;
    for (const [s, z] of band.flat) {
      const [x, y] = toScreen(s, z);
      g.fillRect(x - 1.5, y - 1.5, 3, 3);
    }
    g.setLineDash([]);
    for (const l of band.legs) stroke(g, [[l[0], l[1]], [l[2], l[3]]], null, c.text, 2);
    const look = looks();
    drawStrokes(g, editor.display(), toScreen, look, 1, cave.selected, (band?.stations ?? []).map((s) => [s.s, s.z] as [number, number]));
    drawHandles(g, editor.handles(), c.accent, c.bg);
    if (pickupAt && cave.tool && !live) {
      // the end of an open line the next stroke would carry on from
      g.strokeStyle = c.accent;
      g.lineWidth = 2;
      g.beginPath();
      g.arc(pickupAt[0], pickupAt[1], 9, 0, Math.PI * 2);
      g.stroke();
    }
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
        drawStrokes(g, sec.sketches, to, look, 1, '', [[0, 0]]);
        g.fillStyle = c.muted;
        g.textAlign = 'center';
        g.fillText(sec.label, shell.below[0], shell.below[1]);
        g.textAlign = 'left';
      }
    }
    if (live && live.pts.length > 1) drawStrokes(g, [{ kind: cave.tool || 'sketch', points: live.pts, widths: live.pressure ? live.widths : null, closed: false }], toScreen, look);
    // other people's pointers in this view
    for (const o of cursorsFor('profile', cave.run)) {
      const [ox, oy] = toScreen(o.x, o.y);
      drawCursor(g, ox, oy, o.person?.name ?? '', o.person?.color ?? '#888888');
    }

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
  // Fingers: one draws (with a tool running) or pans, two pan and pinch-zoom together.
  const touches = new Touches();
  const guard = new PenGuard();
  let erasing = false;
  let gesturing = false;
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
    live.widths.push(live.pressure ? widthFor(e.pressure) : 1);
  }

  function onDown(e: PointerEvent) {
    canvas.setPointerCapture(e.pointerId);
    canvas.focus();
    if (e.pointerType === 'pen') {
      guard.pen(e);
      // A pen landing means anything a resting hand started was never a gesture.
      if (touches.count) {
        touches.clear();
        gesturing = false;
        live = null;
        panning = null;
        editor.cancel();
      }
      if (isEraser(e)) {
        erasing = true;
        eraseAt(e);
        return;
      }
    }
    if (e.pointerType === 'touch' && !guard.touchDown(e)) return; // a palm or a fist: ignore it completely
    if (e.pointerType === 'touch' && touches.down(e) >= 2) {
      // A second finger: this was the start of a gesture, not a line.
      gesturing = true;
      live = null;
      panning = null;
      editor.cancel();
      schedule();
      return;
    }
    const middle = e.button === 1;
    if (middle) {
      e.preventDefault();
      if (e.timeStamp - lastMiddle < 350) fit();
      lastMiddle = e.timeStamp;
    }
    if (e.button === 0 && !cave.tool) {
      const r = canvas.getBoundingClientRect();
      if (editor.press(e.clientX - r.left, e.clientY - r.top)) return;
    }
    if (e.button === 0) {
      const rect = canvas.getBoundingClientRect();
      const grab = sectionAt(e.clientX - rect.left, e.clientY - rect.top);
      if (grab) {
        dragSection = { id: grab.id, x: e.clientX, y: e.clientY };
        return;
      }
    }
    const draws = !!cave.tool && !middle && e.button === 0 && (e.pointerType !== 'touch' || option('finger', true)) && !!band;
    if (draws) {
      if (isStampKind(cave.tool)) {
        // A symbol: press where it goes, drag to say which way is up and how big.
        const r = canvas.getBoundingClientRect();
        const p = toWorld(e.clientX - r.left, e.clientY - r.top);
        live = { pts: [p, [p[0], p[1]]], widths: [1, 1], pressure: false };
      } else {
        live = { pts: [], widths: [], pressure: e.pointerType === 'pen' && option('pressure', true) };
        addPoint(e);
      }
    } else {
      panning = { x: e.clientX, y: e.clientY };
    }
  }

  // Where the next stroke would pick an open line up: shown while a drawing tool waits.
  let pickupAt: [number, number] | null = null;
  function showPickup(x: number, y: number) {
    const hit = cave.tool && !isStampKind(cave.tool) ? endpointNear(band?.sketches ?? [], cave.tool, (p) => toScreen(p[0], p[1]), x, y) : null;
    const next = hit ? hit.at : null;
    if ((next?.[0] ?? -1) !== (pickupAt?.[0] ?? -1) || (next?.[1] ?? -1) !== (pickupAt?.[1] ?? -1)) {
      pickupAt = next;
      schedule();
    }
  }

  function eraseAt(e: PointerEvent) {
    const r = canvas.getBoundingClientRect();
    if (editor.eraseAt(e.clientX - r.left, e.clientY - r.top)) schedule();
  }

  function onMove(e: PointerEvent) {
    if (e.pointerType !== 'touch') {
      // tell the others where this pointer is (rate limited; nothing happens when not sharing)
      const rr = canvas.getBoundingClientRect();
      const w = toWorld(e.clientX - rr.left, e.clientY - rr.top);
      reportCursor('profile', cave.run, w[0], w[1], cave.tool);
    }
    if (e.pointerType === 'pen') {
      guard.pen(e);
      if (erasing) {
        eraseAt(e);
        return;
      }
    }
    if (e.pointerType === 'touch' && guard.ignored(e)) return;
    if (e.pointerType === 'touch') {
      const g = touches.move(e);
      if (g) {
        // Two fingers: pan with the middle of them, zoom with how far apart they are.
        const r0 = canvas.getBoundingClientRect();
        const mx = g.cx - r0.left;
        const my = g.cy - r0.top;
        ox = mx - (mx - ox) * g.scale + g.dx;
        oy = my - (my - oy) * g.scale + g.dy;
        scale *= g.scale;
        schedule();
        return;
      }
      if (gesturing) return; // one finger left after a gesture: wait until all are lifted
    }
    const r = canvas.getBoundingClientRect();
    if (editor.move(e.clientX - r.left, e.clientY - r.top)) return;
    if (!live && !panning && cave.tool) showPickup(e.clientX - r.left, e.clientY - r.top);
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
      if (isStampKind(cave.tool)) {
        const r2 = canvas.getBoundingClientRect();
        live.pts[1] = toWorld(e.clientX - r2.left, e.clientY - r2.top);
      } else {
        const samples = e.getCoalescedEvents?.() ?? [];
        for (const ev of samples.length > 0 ? samples : [e]) addPoint(ev);
      }
      schedule();
    } else if (panning) {
      ox += e.clientX - panning.x;
      oy += e.clientY - panning.y;
      panning = { x: e.clientX, y: e.clientY };
      schedule();
    }
  }

  function onUp(e?: PointerEvent) {
    if (e?.pointerType === 'pen') {
      guard.pen(e);
      if (erasing) {
        erasing = false;
        return;
      }
    }
    if (e?.pointerType === 'touch' && guard.ignored(e)) {
      guard.touchUp(e);
      return;
    }
    if (e?.pointerType === 'touch') {
      const left = touches.up(e);
      if (gesturing) {
        // A gesture never commits a stroke; it ends when every finger is up.
        if (left === 0) gesturing = false;
        panning = null;
        return;
      }
      if (e.type === 'pointercancel') {
        // The system took the touch (a palm, a system gesture): drop the stroke.
        live = null;
        panning = null;
        editor.cancel();
        schedule();
        return;
      }
    }
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
    if (isStampKind(cave.tool) && stroked.pts.length === 2 && cave.run) {
      if (cave.tool === 'text' && !option('label', '').trim()) {
        message = 'Type the text in the ribbon first.';
        schedule();
        return;
      }
      const [p0, q] = stroked.pts;
      const a = toScreen(p0[0], p0[1]);
      const b = toScreen(q[0], q[1]);
      // Just a click: a symbol of the chosen size, pointing up.
      const p1: [number, number] = Math.hypot(a[0] - b[0], a[1] - b[1]) < 6 ? [p0[0], p0[1] + option('symbolSize', 1.5)] : q;
      const result = engine.addSketch(cave.run, cave.tool, [p0, p1], null, false, cave.adjusted, option('label', ''));
      message = result === 'ok' ? '' : result;
      changed();
      schedule();
      return;
    }
    if (stroked.pts.length >= 2 && cave.run) {
      // A freehand stroke becomes a smooth curve through a few fit points.
      const wantClosed = option('close', false) || isFillKind(cave.tool); // an area fill is always a loop
      const fit = fitStroke(stroked.pts, stroked.pressure ? stroked.widths : null, tolerancePx(option('detail', 5)) / scale, wantClosed);
      const closed = wantClosed && fit.pts.length >= 3;
      if (isFillKind(cave.tool) && !closed) {
        message = 'A fill needs a loop: draw all the way around the area.';
        schedule();
        return;
      }
      // Starting on the end of an open line of the same kind carries that line on instead of starting a new one.
      const start = toScreen(stroked.pts[0][0], stroked.pts[0][1]);
      const pickup = endpointNear(band?.sketches ?? [], cave.tool, (p) => toScreen(p[0], p[1]), start[0], start[1]);
      let result: string;
      if (pickup) {
        const joined = joinFit(pickup.line, fit, pickup.atStart);
        result = engine.editLine(pickup.line.id, joined.pts, joined.widths, cave.adjusted);
        cave.selected = '';
      } else {
        result = engine.addSketch(cave.run, cave.tool, fit.pts, fit.widths, closed, cave.adjusted);
      }
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
    oncontextmenu={(e) => {
      e.preventDefault();
      if (cave.tool) cave.tool = ''; // a right click ends the command
    }}
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
