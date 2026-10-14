<script lang="ts">
  // One view of the sketchpad (plan, profile or cross section) as a dockable panel. It is the app's own way of
  // drawing, pointed at the survey: the ribbon's Draw tab picks the tool (`cave.tool`), its options tab sets curve
  // detail, close, finger and pressure (`option()`), its Edit tab changes a selected line, `PanelUndo` is the
  // pane's own undo; strokes are fitted with `fitStroke`, drawn with `drawStrokes`, edited with `LineEditor`,
  // and fingers and the pen are told apart by `Touches` and `PenGuard`, exactly as in the Cross Section panel.
  // What is new here is only the grid at a paper scale and the survey's flat views (lib/sketch-grid.ts,
  // lib/sketch-geometry.ts).
  import { onMount, untrack } from 'svelte';
  import { ChevronLeft, ChevronRight, Maximize } from '@lucide/svelte';
  import PanelUndo from '../components/PanelUndo.svelte';
  import { looks } from '../lib/ink';
  import { drawHandles, drawStrokes } from '../lib/section-glyph';
  import { LineEditor, endpointNear, joinFit, type EditableLine } from '../lib/line-editor';
  import { Touches } from '../lib/touch';
  import { PenGuard, isEraser, widthFor } from '../lib/pen';
  import { fitStroke } from '../lib/spline';
  import { tolerancePx } from '../lib/commands';
  import { isStampKind } from '../lib/symbols';
  import { cave, changed, filter, option, setOption } from '../lib/cave.svelte';
  import { orderedScales } from '../lib/sheet.svelte';
  import { engine, loadEngine, type SheetOptions } from '../lib/engine';
  import { formatLength } from '../lib/units';
  import { anchorStroke, fitView, placeStroke, toScreen as project, toWorld as unproject, type Pt, type Transform } from '../lib/sketch-geometry';
  import { GRID, describeScale, drawGrid, gridCaption, matchScale, pxPerMetre, visibleGrid } from '../lib/sketch-grid';
  import { PANE_SCOPE, PANE_TITLE, sketch, type Pane } from '../lib/sketch.svelte';

  let { pane }: { pane: Pane } = $props();

  const view = $derived(sketch.views[pane]);
  const stations = $derived(sketch.stationsOf(pane));
  let canvas: HTMLCanvasElement;
  let wrap: HTMLDivElement;
  let scales = $state<SheetOptions['scales']>([]);
  let message = $state('');
  let live: { pts: Pt[]; widths: number[]; pressure: boolean } | null = null;
  let frame = 0;
  onMount(() => {
    loadEngine().then(() => (scales = orderedScales(engine.sheetOptions().scales, cave.units)));
  });

  /** The pane's lines as the line editor wants them: points in the pane's metres. */
  type Line = EditableLine & { kind: string; closed: boolean; text: string; flip: boolean };
  const lines = $derived.by<Line[]>(() => {
    const out: Line[] = [];
    for (const s of sketch.paneStrokes(pane)) {
      const points = placeStroke(stations, s);
      if (points) out.push({ id: s.id, points, widths: s.widths, closed: s.closed, kind: s.kind, text: s.text, flip: s.flip });
    }
    return out;
  });

  const tf = (): Transform => sketch.views[pane].tf;
  const toScreen = (p: Pt): Pt => project(tf(), p);
  const toWorld = (x: number, y: number): Pt => unproject(tf(), [x, y]);

  // Drawn lines can be selected and their fit points moved, added and deleted (the app's one implementation).
  const editor = new LineEditor<Line>({
    lines: () => lines,
    toScreen,
    toWorld,
    selected: () => cave.selected,
    select: (id) => (cave.selected = id),
    commit: (id, pts, widths) => sketch.setPoints(id, pts, widths),
    remove: (id) => sketch.remove(id),
    redraw: () => schedule(),
    hidden: (kind) => !!kind && filter.hiddenTypes.includes(kind),
  });
  $effect(() => {
    void cave.selected;
    schedule();
  });
  // the selected line is fitted again at another detail (the Edit tab's slider)
  let handledRefit = cave.refit.seq; // a panel opened later does not redo old requests
  $effect(() => {
    const r = cave.refit;
    if (r.seq === handledRefit) return;
    handledRefit = r.seq;
    untrack(() => editor.refitLine(r.id, r.detail));
  });

  // ---- framing: at a paper scale, on the survey, until the person moves the pane ----

  function fitPoints(): Pt[] {
    if (pane === 'plan') return [...sketch.plan.stations.map((s) => s.p), ...sketch.plan.splays.map((s) => s[1]), ...sketch.plan.walls.flat()];
    if (pane === 'profile') return [...sketch.profile.stations.map((s) => s.p), ...sketch.profile.walls.flat()];
    const s = sketch.section;
    return s ? [[0, 0], ...(s.outline ?? []), ...s.splays] : [];
  }

  /** The point kept in the middle at a fixed scale: the station stood at; for a section, the middle of the passage. */
  function centre(): Pt {
    if (pane === 'section') {
      const o = sketch.section?.outline;
      return o ? [(o[2][0] + o[0][0]) / 2, (o[1][1] + o[3][1]) / 2] : [0, 0];
    }
    const label = sketch.standing ?? sketch.selected;
    const s = stations.find((x) => x.label === label) ?? stations[stations.length - 1];
    return s ? s.p : [0, 0];
  }

  $effect(() => {
    void sketch.L;
    void sketch.plan;
    void sketch.profile;
    void sketch.section;
    void sketch.standing;
    void sketch.selected;
    const { w, h } = view.size;
    const scale = view.scale;
    if (view.frame !== 'auto' || w === 0) return;
    untrack(() => {
      if (scale === 'fit') {
        sketch.views[pane].tf = fitView(fitPoints(), w, h, 30, pane === 'section' ? 140 : 90);
        return;
      }
      const k = pxPerMetre(scale);
      const c = centre();
      sketch.views[pane].tf = { scale: k, tx: w / 2 - c[0] * k, ty: h / 2 + c[1] * k };
    });
  });

  const selectValue = $derived(view.frame === 'auto' ? String(view.scale) : String(matchScale(view.tf.scale, scales)?.feet_per_inch ?? 'custom'));
  function chooseScale(v: string) {
    if (v === 'custom') return;
    sketch.views[pane].scale = v === 'fit' ? 'fit' : Number(v);
    sketch.views[pane].frame = 'auto';
    sketch.save();
  }
  const fitAll = () => chooseScale('fit');
  const reframe = () => (sketch.views[pane].frame = 'auto');
  const free = () => (sketch.views[pane].frame = 'free');

  // ---- drawing ----

  function schedule() {
    if (!frame) frame = requestAnimationFrame(() => ((frame = 0), draw()));
  }
  $effect(() => {
    void sketch.L; void sketch.plan; void sketch.profile; void sketch.section;
    void sketch.strokes; void sketch.standing; void sketch.selected;
    void view.tf; void view.size; void lines; void scales;
    schedule();
  });

  const css = () => {
    const c = getComputedStyle(document.documentElement);
    const v = (n: string) => c.getPropertyValue(n).trim();
    return { text: v('--text'), muted: v('--muted'), line: v('--line'), accent: v('--accent'), accentBg: v('--accent-bg'), panel: v('--panel'), warn: v('--warn') };
  };

  function poly(g: CanvasRenderingContext2D, pts: Pt[], close = false) {
    g.beginPath();
    pts.forEach((p, i) => {
      const [x, y] = toScreen(p);
      if (i === 0) g.moveTo(x, y);
      else g.lineTo(x, y);
    });
    if (close) g.closePath();
  }

  function draw() {
    const { w, h } = view.size;
    if (!canvas || w === 0) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    const g = canvas.getContext('2d')!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, w, h);
    const c = css();
    const t = tf();
    g.font = '11px system-ui, sans-serif';
    drawGrid(g, t, w, h, c.text);

    const dot = (p: Pt, label: string) => {
      const [x, y] = toScreen(p);
      g.fillStyle = c.text;
      g.beginPath();
      g.arc(x, y, 3.5, 0, Math.PI * 2);
      g.fill();
      g.fillStyle = c.muted;
      g.fillText(label, x + 6, y - 6);
    };
    const ring = (p: Pt, color: string, r: number) => {
      const [x, y] = toScreen(p);
      g.strokeStyle = color;
      g.lineWidth = 2;
      g.beginPath();
      g.arc(x, y, r, 0, Math.PI * 2);
      g.stroke();
    };
    const wash = (shapes: Pt[][]) => {
      g.save();
      g.globalAlpha = 0.4;
      g.fillStyle = c.accentBg;
      for (const s of shapes) {
        poly(g, s, true);
        g.fill();
      }
      g.restore();
    };

    if (pane === 'plan' || pane === 'profile') {
      const v = pane === 'plan' ? sketch.plan : sketch.profile;
      wash(v.passage);
      g.strokeStyle = c.muted;
      g.lineWidth = 1.5;
      g.setLineDash([6, 4]); // measured walls: a guide to trace against, never the passage itself
      for (const wall of v.walls) {
        poly(g, wall);
        g.stroke();
      }
      g.setLineDash([]);
      if (pane === 'plan') {
        g.lineWidth = 1;
        for (const [a, b] of sketch.plan.splays) {
          poly(g, [a, b]);
          g.stroke();
        }
      }
      g.strokeStyle = c.accent;
      g.lineWidth = 2;
      for (const [a, b] of v.legs) {
        poly(g, [a, b]);
        g.stroke();
      }
      for (const s of v.stations) dot(s.p, s.label);
      const here = v.stations.find((s) => s.label === sketch.standing);
      if (here) ring(here.p, c.warn, 9);
      const sel = v.stations.find((s) => s.label === sketch.selected);
      if (sel && sketch.selected !== sketch.standing) ring(sel.p, c.accent, 8);
      if (pane === 'plan') {
        g.fillStyle = c.muted;
        g.strokeStyle = c.muted;
        g.lineWidth = 1.5;
        g.fillText('N', w - 22, 18);
        g.beginPath();
        g.moveTo(w - 18, 40);
        g.lineTo(w - 18, 24);
        g.lineTo(w - 22, 30);
        g.moveTo(w - 18, 24);
        g.lineTo(w - 14, 30);
        g.stroke();
      }
    } else if (sketch.section) {
      const s = sketch.section;
      const [cx, cy] = toScreen([0, 0]);
      g.strokeStyle = c.text;
      g.lineWidth = 1.6;
      g.beginPath();
      g.moveTo(cx - 9, cy + 7);
      g.lineTo(cx + 9, cy + 7);
      g.lineTo(cx, cy - 8);
      g.closePath();
      g.stroke();
      if (s.outline) {
        wash([s.outline]);
        g.setLineDash([6, 4]);
        g.strokeStyle = c.muted;
        g.lineWidth = 1.5;
        g.lineJoin = 'round';
        poly(g, s.outline, true);
        g.stroke();
        g.setLineDash([]);
        const [l, u, r, d] = s.outline;
        g.fillStyle = c.text;
        g.fillText(`L ${formatLength(Math.abs(l[0]), 'imperial', 1)}   R ${formatLength(r[0], 'imperial', 1)}   U ${formatLength(u[1], 'imperial', 1)}   D ${formatLength(Math.abs(d[1]), 'imperial', 1)}`, 12, 34);
      }
      g.fillStyle = c.muted;
      for (const p of s.splays) {
        const [x, y] = toScreen(p);
        g.beginPath();
        g.arc(x, y, 3.2, 0, Math.PI * 2);
        g.fill();
      }
      g.fillText(`${s.station}: looking along the passage`, 12, 18);
    } else {
      g.fillStyle = c.muted;
      g.fillText('No station yet.', 12, 18);
    }

    // what has been sketched: the app's own stroke drawing, with selection halo and handles
    const look = looks();
    const to = (u: number, v: number): [number, number] => toScreen([u, v]);
    drawStrokes(g, editor.display(), to, look, 1, cave.selected, stations.map((s) => s.p));
    drawHandles(g, editor.handles(), c.accent, c.panel);
    if (pickupAt && cave.tool && !live) {
      g.strokeStyle = c.accent; // the end of an open line the next stroke would carry on from
      g.lineWidth = 2;
      g.beginPath();
      g.arc(pickupAt[0], pickupAt[1], 9, 0, Math.PI * 2);
      g.stroke();
    }
    if (live && live.pts.length > 1) drawStrokes(g, [{ kind: cave.tool || 'sketch', points: live.pts, widths: live.pressure ? live.widths : null, closed: false }], to, look);

    // what the scale is, and the grid: a bar one heavy square long
    const bar = GRID.majorM * t.scale;
    g.strokeStyle = c.text;
    g.fillStyle = c.muted;
    g.lineWidth = 2;
    if (bar < w - 30 && visibleGrid(t.scale).major) {
      g.beginPath();
      g.moveTo(16, h - 14);
      g.lineTo(16 + bar, h - 14);
      g.stroke();
    }
    const fine = visibleGrid(t.scale).minor ? '' : ' (fine lines hidden: zoom in)';
    g.fillText(`${describeScale(t.scale, scales)}  ·  grid ${gridCaption()}${fine}`, 16, h - 22);
    if (message) {
      g.fillStyle = c.warn;
      g.fillText(message, 12, 50);
    }
  }

  // ---- the canvas's size ----
  onMount(() => {
    const apply = () => {
      const r = wrap.getBoundingClientRect();
      sketch.views[pane].size = { w: Math.floor(r.width), h: Math.floor(r.height) };
    };
    const ro = new ResizeObserver(apply);
    ro.observe(wrap);
    apply();
    return () => ro.disconnect();
  });

  // ---- input: pen and mouse draw, fingers and the middle button pan, the wheel zooms (the app's rules) ----
  const touches = new Touches();
  const guard = new PenGuard();
  let erasing = false;
  let gesturing = false;
  let panning: { x: number; y: number } | null = null;
  let lastMiddle = 0;
  let moved = 0;
  let pressedAt: Pt | null = null;

  function addPoint(e: PointerEvent) {
    if (!live) return;
    const rect = canvas.getBoundingClientRect();
    const p = toWorld(e.clientX - rect.left, e.clientY - rect.top);
    const last = live.pts[live.pts.length - 1];
    if (last && Math.hypot(p[0] - last[0], p[1] - last[1]) * tf().scale < 1.5) return;
    live.pts.push(p);
    // Pressure only changes how thick the line is drawn, never where it goes.
    live.widths.push(live.pressure ? widthFor(e.pressure) : 1);
  }

  function onDown(e: PointerEvent) {
    try {
      canvas.setPointerCapture(e.pointerId);
    } catch {
      /* the pointer is already gone: carry on without capture */
    }
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
    if (e.pointerType === 'touch' && !guard.touchDown(e)) {
      engine.log('warn', 'touch', `A touch was ignored: ${guard.lastRejected}.`);
      return; // a palm or a fist: ignore it completely
    }
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
      if (e.timeStamp - lastMiddle < 350) fitAll();
      lastMiddle = e.timeStamp;
    }
    const r = canvas.getBoundingClientRect();
    moved = 0;
    pressedAt = [e.clientX - r.left, e.clientY - r.top];
    if (e.button === 0 && !cave.tool) {
      if (editor.press(e.clientX - r.left, e.clientY - r.top)) return;
    }
    // Symbols and text are placed by the real panels; the sketchpad draws lines.
    const draws = !!cave.tool && !isStampKind(cave.tool) && !middle && e.button === 0 && (e.pointerType !== 'touch' || option('finger', true));
    if (draws) {
      live = { pts: [], widths: [], pressure: e.pointerType === 'pen' && option('pressure', true) };
      addPoint(e);
    } else {
      panning = { x: e.clientX, y: e.clientY };
      free();
    }
  }

  // Where the next stroke would pick an open line up: shown while a drawing tool waits.
  let pickupAt: Pt | null = null;
  function showPickup(x: number, y: number) {
    const hit = cave.tool && !isStampKind(cave.tool) ? endpointNear(lines, cave.tool, toScreen, x, y) : null;
    const at = hit ? toScreen(hit.at) : null;
    if ((at?.[0] ?? -1) !== (pickupAt?.[0] ?? -1) || (at?.[1] ?? -1) !== (pickupAt?.[1] ?? -1)) {
      pickupAt = at;
      schedule();
    }
  }

  function eraseAt(e: PointerEvent) {
    const r = canvas.getBoundingClientRect();
    if (editor.eraseAt(e.clientX - r.left, e.clientY - r.top)) schedule();
  }

  function onMove(e: PointerEvent) {
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
        const t = tf();
        sketch.views[pane].tf = { scale: t.scale * g.scale, tx: mx - (mx - t.tx) * g.scale + g.dx, ty: my - (my - t.ty) * g.scale + g.dy };
        free();
        schedule();
        return;
      }
      if (gesturing) return; // one finger left after a gesture: wait until all are lifted
    }
    const r = canvas.getBoundingClientRect();
    if (pressedAt) moved = Math.max(moved, Math.hypot(e.clientX - r.left - pressedAt[0], e.clientY - r.top - pressedAt[1]));
    if (editor.move(e.clientX - r.left, e.clientY - r.top)) return;
    if (!live && !panning && cave.tool) showPickup(e.clientX - r.left, e.clientY - r.top);
    if (live) {
      const samples = e.getCoalescedEvents?.() ?? [];
      for (const ev of samples.length > 0 ? samples : [e]) addPoint(ev);
      schedule();
    } else if (panning) {
      const t = tf();
      sketch.views[pane].tf = { ...t, tx: t.tx + (e.clientX - panning.x), ty: t.ty + (e.clientY - panning.y) };
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
    const wasPanning = !!panning;
    panning = null;
    if (editor.release()) return;
    if (wasPanning && !cave.tool && moved < 4 && pressedAt) pickStation(pressedAt); // a tap on a station, with no tool: the cross section goes there
    if (!live) return;
    const stroked = live;
    live = null;
    if (stroked.pts.length >= 2) {
      // A freehand stroke becomes a smooth curve through a few fit points.
      const wantClosed = option('close', false);
      const fit = fitStroke(stroked.pts, stroked.pressure ? stroked.widths : null, tolerancePx(option('detail', 5)) / tf().scale, wantClosed);
      const closed = wantClosed && fit.pts.length >= 3;
      // Starting on the end of an open line of the same kind carries that line on instead of starting a new one.
      const start = toScreen(stroked.pts[0]);
      const pickup = endpointNear(lines, cave.tool, toScreen, start[0], start[1]);
      if (pickup) {
        const joined = joinFit(pickup.line, fit, pickup.atStart);
        sketch.setPoints(pickup.line.id, joined.pts, joined.widths);
        cave.selected = '';
      } else {
        const hung = anchorStroke(stations, fit.pts, cave.tool, fit.widths);
        if (hung) {
          message = '';
          sketch.add(pane, hung, closed);
        } else message = 'There is no station to hang the line on yet.';
      }
      changed();
    }
    schedule();
  }

  /** A tap near a station in the plan or profile puts the cross section there (and stops it following the survey). */
  function pickStation(p: Pt) {
    if (pane === 'section') return;
    let best: string | null = null;
    let bd = 22;
    for (const s of stations) {
      const [x, y] = toScreen(s.p);
      const d = Math.hypot(x - p[0], y - p[1]);
      if (d < bd) {
        bd = d;
        best = s.label;
      }
    }
    if (best) {
      setOption('followSurvey', false);
      cave.station = best;
    }
  }

  function stepStation(by: number) {
    const to = sketch.neighbour(by);
    if (!to) return;
    setOption('followSurvey', false);
    cave.station = to;
  }

  function onWheel(e: WheelEvent) {
    e.preventDefault();
    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const f = Math.exp(-e.deltaY * 0.0015);
    const t = tf();
    sketch.views[pane].tf = { scale: t.scale * f, tx: mx - (mx - t.tx) * f, ty: my - (my - t.ty) * f };
    free();
    schedule();
  }

  function onKey(e: KeyboardEvent) {
    if (editor.key(e)) {
      e.preventDefault();
      return;
    }
    if (!(e.metaKey || e.ctrlKey) || e.key.toLowerCase() !== 'z') return;
    e.preventDefault();
    if (e.shiftKey ? engine.redo(PANE_SCOPE[pane]) : engine.undo(PANE_SCOPE[pane])) changed();
  }
</script>

<section class="pane" aria-label={PANE_TITLE[pane]}>
  <header>
    <PanelUndo scope={PANE_SCOPE[pane]} label="in the {PANE_TITLE[pane]}" />
    {#if pane === 'section'}
      <button class="ic" title="Previous station" aria-label="Previous station" onclick={() => stepStation(-1)}><ChevronLeft size={15} /></button>
      <span class="st">{sketch.selected ?? 'no station'}</span>
      <button class="ic" title="Next station" aria-label="Next station" onclick={() => stepStation(1)}><ChevronRight size={15} /></button>
    {:else}
      <span class="hint">tap a station to see its section</span>
    {/if}
    <span class="sp"></span>
    <select value={selectValue} aria-label="{PANE_TITLE[pane]} scale" title="Drawing scale: inches of paper to feet of ground (the same list as the Sheets tab)" onchange={(e) => chooseScale(e.currentTarget.value)}>
      <option value="fit">Fit survey</option>
      {#each scales as s}<option value={String(s.feet_per_inch)}>{s.label}</option>{/each}
      {#if selectValue === 'custom'}<option value="custom" disabled>{describeScale(view.tf.scale, scales)}</option>{/if}
    </select>
    <button class="ic" class:on={view.frame === 'auto' && view.scale === 'fit'} title="Fit the whole survey" aria-label="Fit {PANE_TITLE[pane]} to the survey" onclick={fitAll}><Maximize size={15} /></button>
  </header>
  <div class="canvas" bind:this={wrap}>
    <canvas
      bind:this={canvas}
      tabindex="0"
      aria-label="{PANE_TITLE[pane]} drawing surface"
      style="cursor:{cave.tool ? 'crosshair' : 'grab'}"
      onpointerdown={onDown}
      onpointermove={onMove}
      onpointerup={onUp}
      onpointercancel={onUp}
      onwheel={onWheel}
      ondblclick={(e) => {
        if (cave.tool) return;
        const r = canvas.getBoundingClientRect();
        if (!editor.addPoint(e.clientX - r.left, e.clientY - r.top)) reframe();
      }}
      oncontextmenu={(e) => {
        e.preventDefault();
        if (cave.tool) cave.tool = ''; // a right click ends the command
      }}
      onkeydown={onKey}
    ></canvas>
  </div>
</section>

<style>
  .pane { display: flex; flex-direction: column; height: 100%; min-height: 0; min-width: 0; background: var(--panel); color: var(--text); font-size: 12px; }
  header { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 4px 6px; border-bottom: 1px solid var(--line); min-height: 34px; }
  header .hint { color: var(--muted); font-size: 11px; }
  header .st { min-width: 3.5em; text-align: center; font-weight: 600; }
  .sp { flex: 1; }
  select { font: inherit; font-size: 12px; padding: 4px 6px; min-height: 30px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); }
  button.ic { display: inline-flex; align-items: center; justify-content: center; width: 32px; height: 30px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); cursor: pointer; padding: 0; touch-action: manipulation; }
  button.ic.on { border-color: var(--accent); color: var(--accent); }
  .canvas { flex: 1; min-height: 0; position: relative; }
  canvas { position: absolute; inset: 0; width: 100%; height: 100%; display: block; touch-action: none; outline: none; }
</style>
