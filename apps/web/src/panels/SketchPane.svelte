<script module lang="ts">
  // A pen in use makes any finger a palm, whichever pane it lands on: shared by every pane.
  let penUntil = 0;
</script>

<script lang="ts">
  // One view of the sketchpad (plan, profile or cross section) as a dockable panel. Graph-paper grid at a
  // paper scale (1" = 20' to start with, a dropdown of the usual ones), pen to draw, fingers to pan and pinch,
  // wheel to zoom, strokes hanging on stations. The three panes share one store (lib/sketch.svelte.ts), so
  // they can be placed, tabbed, floated and resized like any other panel.
  import { untrack } from 'svelte';
  import { ChevronLeft, ChevronRight, Maximize, Redo2, Undo2 } from '@lucide/svelte';
  import { anchorStroke, fitView, placeStroke, toScreen, toWorld, type Anchored, type Pt, type Transform } from '../lib/sketch-geometry';
  import { SCALES, describeScale, gridFor, gridLines, matchScale, stepText } from '../lib/sketch-scale';
  import { PANE_TITLE, sketch, type Pane } from '../lib/sketch.svelte';

  let { pane }: { pane: Pane } = $props();

  const view = $derived(sketch.views[pane]);
  let canvas = $state<HTMLCanvasElement | null>(null);
  let live = $state<{ pts: Pt[]; w: number[]; kind: string } | null>(null);

  const stations = $derived(pane === 'plan' ? sketch.plan.stations : pane === 'profile' ? sketch.profile.stations : sketch.selected ? [{ label: sketch.selected, p: [0, 0] as Pt }] : []);

  /** Everything the pane should show when fitted. */
  function fitPoints(): Pt[] {
    if (pane === 'plan') return [...sketch.plan.stations.map((s) => s.p), ...sketch.plan.splays.map((s) => s[1]), ...sketch.plan.walls.flat()];
    if (pane === 'profile') return [...sketch.profile.stations.map((s) => s.p), ...sketch.profile.walls.flat()];
    const s = sketch.section;
    return s ? [[0, 0], ...(s.outline ?? []), ...s.splays] : [];
  }

  /** The point to keep in the middle of the pane at a fixed scale: the station stood at (or picked); for a section, the middle of the passage. */
  function centre(): Pt {
    if (pane === 'section') {
      const o = sketch.section?.outline;
      return o ? [(o[2][0] + o[0][0]) / 2, (o[1][1] + o[3][1]) / 2] : [0, 0];
    }
    const label = sketch.standing ?? sketch.selected;
    const s = stations.find((x) => x.label === label) ?? stations[stations.length - 1];
    return s ? s.p : [0, 0];
  }

  const presetOf = (id: string) => SCALES.find((s) => s.id === id);

  // Frame itself on the survey until the person moves the pane.
  $effect(() => {
    void sketch.L;
    void sketch.plan;
    void sketch.profile;
    void sketch.section;
    void sketch.standing;
    void sketch.selected;
    const { w, h } = view.size;
    const id = view.scaleId;
    if (view.frame !== 'auto' || w === 0) return;
    untrack(() => {
      const preset = presetOf(id);
      if (!preset) {
        sketch.views[pane].tf = fitView(fitPoints(), w, h, 30, pane === 'section' ? 140 : 90);
        return;
      }
      const c = centre();
      sketch.views[pane].tf = { scale: preset.pxPerM, tx: w / 2 - c[0] * preset.pxPerM, ty: h / 2 + c[1] * preset.pxPerM };
    });
  });

  // ---- the scale control ----
  const selectValue = $derived(view.frame === 'auto' ? view.scaleId : (matchScale(view.tf.scale)?.id ?? 'custom'));
  function chooseScale(id: string) {
    if (id === 'custom') return;
    sketch.views[pane].scaleId = id;
    sketch.views[pane].frame = 'auto';
    sketch.save();
  }
  function reframe() {
    sketch.views[pane].frame = 'auto';
  }
  function fitAll() {
    sketch.views[pane].scaleId = 'fit';
    sketch.views[pane].frame = 'auto';
    sketch.save();
  }
  const free = () => (sketch.views[pane].frame = 'free');

  // ---- drawing ----
  let raf = 0;
  function redraw() {
    cancelAnimationFrame(raf);
    raf = requestAnimationFrame(draw);
  }
  $effect(() => {
    void sketch.L; void sketch.plan; void sketch.profile; void sketch.section;
    void sketch.strokes; void sketch.standing; void sketch.selected; void sketch.tool;
    void view.tf; void view.size; void live; void canvas;
    redraw();
  });

  const css = (el: HTMLElement, name: string, fallback: string) => getComputedStyle(el).getPropertyValue(name).trim() || fallback;

  function poly(ctx: CanvasRenderingContext2D, t: Transform, pts: Pt[], close = false) {
    ctx.beginPath();
    pts.forEach((p, i) => {
      const [x, y] = toScreen(t, p);
      if (i === 0) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
    });
    if (close) ctx.closePath();
  }

  /** A stroke as a smooth curve through its points (midpoint quadratics), width scaled by pen weight. */
  function ink(ctx: CanvasRenderingContext2D, t: Transform, pts: Pt[], w: number[] | undefined, kind: string, colors: { text: string; accent: string }) {
    if (pts.length === 0) return;
    const s = pts.map((p) => toScreen(t, p));
    ctx.save();
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    ctx.strokeStyle = kind === 'note' ? colors.accent : colors.text;
    ctx.setLineDash(kind === 'ledge' ? [7, 5] : []);
    const base = kind === 'note' ? 1.6 : 2.2;
    if (s.length === 1) {
      ctx.beginPath();
      ctx.arc(s[0][0], s[0][1], base, 0, Math.PI * 2);
      ctx.fillStyle = ctx.strokeStyle;
      ctx.fill();
    } else if (!w || w.every((x) => Math.abs(x - w[0]) < 0.05)) {
      ctx.lineWidth = base * (w?.[0] ?? 1);
      ctx.beginPath();
      ctx.moveTo(s[0][0], s[0][1]);
      for (let i = 1; i < s.length - 1; i++) ctx.quadraticCurveTo(s[i][0], s[i][1], (s[i][0] + s[i + 1][0]) / 2, (s[i][1] + s[i + 1][1]) / 2);
      ctx.lineTo(s[s.length - 1][0], s[s.length - 1][1]);
      ctx.stroke();
    } else {
      // Pressure changes the weight of the line, never where it is.
      for (let i = 1; i < s.length; i++) {
        ctx.lineWidth = base * ((w[i] + w[i - 1]) / 2);
        ctx.beginPath();
        ctx.moveTo(s[i - 1][0], s[i - 1][1]);
        ctx.lineTo(s[i][0], s[i][1]);
        ctx.stroke();
      }
    }
    ctx.restore();
  }

  function drawGrid(ctx: CanvasRenderingContext2D, t: Transform, w: number, h: number, c: { text: string; muted: string }) {
    const { minorM, majorM } = gridFor(t.scale, 7.5, describeScale(t.scale).startsWith('1"'));
    const [x0, y1] = toWorld(t, [0, 0]);
    const [x1, y0] = toWorld(t, [w, h]);
    const minorPx = minorM * t.scale;
    ctx.save();
    ctx.lineWidth = 1;
    ctx.strokeStyle = c.text;
    const paint = (heavy: boolean) => {
      ctx.globalAlpha = heavy ? 0.2 : 0.075;
      ctx.beginPath();
      for (const l of gridLines(x0, x1, minorPx < 4 ? majorM : minorM, majorM)) {
        if (l.heavy !== heavy) continue;
        const x = Math.round(t.tx + l.at * t.scale) + 0.5;
        ctx.moveTo(x, 0);
        ctx.lineTo(x, h);
      }
      for (const l of gridLines(y0, y1, minorPx < 4 ? majorM : minorM, majorM)) {
        if (l.heavy !== heavy) continue;
        const y = Math.round(t.ty - l.at * t.scale) + 0.5;
        ctx.moveTo(0, y);
        ctx.lineTo(w, y);
      }
      ctx.stroke();
    };
    paint(false);
    paint(true);
    ctx.restore();
  }

  function draw() {
    const cv = canvas;
    const { w, h } = view.size;
    if (!cv || w === 0) return;
    const ctx = cv.getContext('2d');
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const c = { text: css(cv, '--text', '#ddd'), muted: css(cv, '--muted', '#888'), accent: css(cv, '--accent', '#4af'), warn: css(cv, '--warn', '#fb3'), line: css(cv, '--line', '#444') };
    const t = view.tf;
    ctx.font = '11px system-ui, sans-serif';
    drawGrid(ctx, t, w, h, c);

    const dot = (p: Pt, label: string, r = 3.5) => {
      const [x, y] = toScreen(t, p);
      ctx.fillStyle = c.text;
      ctx.beginPath();
      ctx.arc(x, y, r, 0, Math.PI * 2);
      ctx.fill();
      ctx.fillStyle = c.muted;
      ctx.fillText(label, x + 6, y - 6);
    };
    const ring = (p: Pt, color: string, r: number) => {
      const [x, y] = toScreen(t, p);
      ctx.strokeStyle = color;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(x, y, r, 0, Math.PI * 2);
      ctx.stroke();
    };

    if (pane === 'plan' || pane === 'profile') {
      const v = pane === 'plan' ? sketch.plan : sketch.profile;
      ctx.save();
      ctx.globalAlpha = 0.16;
      ctx.fillStyle = c.muted;
      for (const poly_ of v.passage) {
        poly(ctx, t, poly_, true);
        ctx.fill();
      }
      ctx.restore();
      ctx.strokeStyle = c.text;
      ctx.lineWidth = 1.4;
      ctx.setLineDash([]);
      for (const wall of v.walls) {
        poly(ctx, t, wall);
        ctx.stroke();
      }
      if (pane === 'plan') {
        ctx.strokeStyle = c.muted;
        ctx.lineWidth = 1;
        for (const [a, b] of sketch.plan.splays) {
          poly(ctx, t, [a, b]);
          ctx.stroke();
        }
      }
      ctx.strokeStyle = c.accent;
      ctx.lineWidth = 2;
      for (const [a, b] of v.legs) {
        poly(ctx, t, [a, b]);
        ctx.stroke();
      }
      for (const s of v.stations) dot(s.p, s.label);
      const here = v.stations.find((s) => s.label === sketch.standing);
      if (here) ring(here.p, c.warn, 9);
      const sel = v.stations.find((s) => s.label === sketch.selected);
      if (sel && sketch.selected !== sketch.standing) ring(sel.p, c.accent, 8);
      if (pane === 'plan') {
        ctx.fillStyle = c.muted;
        ctx.fillText('N', w - 22, 18);
        ctx.strokeStyle = c.muted;
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.moveTo(w - 18, 40);
        ctx.lineTo(w - 18, 24);
        ctx.lineTo(w - 22, 30);
        ctx.moveTo(w - 18, 24);
        ctx.lineTo(w - 14, 30);
        ctx.stroke();
      }
    } else if (sketch.section) {
      const section = sketch.section;
      const [ox, oy] = toScreen(t, [0, 0]);
      ctx.strokeStyle = c.line;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(ox - 12, oy);
      ctx.lineTo(ox + 12, oy);
      ctx.moveTo(ox, oy - 12);
      ctx.lineTo(ox, oy + 12);
      ctx.stroke();
      if (section.outline) {
        ctx.save();
        ctx.globalAlpha = 0.16;
        ctx.fillStyle = c.muted;
        poly(ctx, t, section.outline, true);
        ctx.fill();
        ctx.restore();
        ctx.strokeStyle = c.text;
        ctx.lineWidth = 1.4;
        poly(ctx, t, section.outline, true);
        ctx.stroke();
        // The dimensions in one line, where they cannot pile up on a small section.
        const [l, u, r, d] = section.outline;
        const val = (n: number) => `${+Math.abs(n).toFixed(2)}`;
        ctx.fillStyle = c.text;
        ctx.fillText(`L ${val(l[0])}   R ${val(r[0])}   U ${val(u[1])}   D ${val(d[1])} m`, 12, 34);
      }
      ctx.fillStyle = c.accent;
      for (const p of section.splays) {
        const [x, y] = toScreen(t, p);
        ctx.beginPath();
        ctx.arc(x, y, 3, 0, Math.PI * 2);
        ctx.fill();
      }
      ctx.fillStyle = c.muted;
      ctx.fillText(`looking along the passage at ${section.station}`, 12, 18);
    } else {
      ctx.fillStyle = c.muted;
      ctx.fillText('No station yet.', 12, 18);
    }

    // sketch strokes, then the one being drawn
    for (const a of sketch.strokesOf(pane)) {
      const pts = placeStroke(stations, a);
      if (pts) ink(ctx, t, pts, a.w, a.kind, c);
    }
    if (live) ink(ctx, t, live.pts, live.w, live.kind, c);

    // scale: a bar the length of one heavy grid square, and what the grid is
    const imperial = describeScale(t.scale).startsWith('1"');
    const g = gridFor(t.scale, 7.5, imperial);
    const barPx = g.majorM * t.scale;
    ctx.strokeStyle = c.muted;
    ctx.fillStyle = c.muted;
    ctx.lineWidth = 2;
    if (barPx < w - 30) {
      ctx.beginPath();
      ctx.moveTo(12, h - 14);
      ctx.lineTo(12 + barPx, h - 14);
      ctx.moveTo(12, h - 18);
      ctx.lineTo(12, h - 10);
      ctx.moveTo(12 + barPx, h - 18);
      ctx.lineTo(12 + barPx, h - 10);
      ctx.stroke();
    }
    ctx.fillText(`${describeScale(t.scale)}  ·  grid ${stepText(g.minorM, imperial)} / ${stepText(g.majorM, imperial)}`, 12, h - 22);
  }

  // ---- the canvas's size ----
  function measure(node: HTMLCanvasElement) {
    canvas = node;
    const apply = () => {
      const r = node.parentElement!.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      const w = Math.max(0, Math.floor(r.width));
      const h = Math.max(0, Math.floor(r.height));
      node.width = Math.max(1, Math.floor(w * dpr));
      node.height = Math.max(1, Math.floor(h * dpr));
      node.style.width = `${w}px`;
      node.style.height = `${h}px`;
      sketch.views[pane].size = { w, h };
    };
    const ro = new ResizeObserver(apply);
    ro.observe(node.parentElement!);
    apply();
    return { destroy: () => ro.disconnect() };
  }

  // ---- pointer input ----
  const touches = new Map<number, { x: number; y: number }>();
  let drawing: { id: number; before: string; erase: boolean; moved: number } | null = null;
  let panning: { id: number; x: number; y: number } | null = null;

  const local = (e: { clientX: number; clientY: number }): Pt => {
    const r = canvas!.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  };
  const weight = (e: PointerEvent) => (e.pointerType === 'pen' ? Math.min(1.8, 0.45 + e.pressure * 1.2) : 1);
  const setTf = (t: Transform) => (sketch.views[pane].tf = t);

  function down(e: PointerEvent) {
    try {
      canvas!.setPointerCapture(e.pointerId);
    } catch {
      /* the pointer is already gone: carry on without capture */
    }
    const p = local(e);
    if (e.pointerType === 'touch') {
      // Fingers move the view. A pen stroke in progress (or just finished) means this is a palm.
      if (drawing || performance.now() < penUntil) return;
      touches.set(e.pointerId, { x: p[0], y: p[1] });
      free();
      return;
    }
    if (e.pointerType === 'pen') penUntil = performance.now() + 700;
    const eraserEnd = e.pointerType === 'pen' && (e.buttons & 32) !== 0;
    if (e.button === 1 || (sketch.tool === 'pan' && !eraserEnd)) {
      panning = { id: e.pointerId, x: p[0], y: p[1] };
      free();
      return;
    }
    if (e.button !== 0 && e.pointerType === 'mouse') return;
    const erase = sketch.tool === 'erase' || eraserEnd;
    drawing = { id: e.pointerId, before: sketch.snapshot(pane), erase, moved: 0 };
    if (!erase) live = { pts: [toWorld(view.tf, p)], w: [weight(e)], kind: sketch.tool === 'note' ? 'note' : sketch.tool === 'ledge' ? 'ledge' : 'wall' };
    else eraseAt(p);
  }

  function move(e: PointerEvent) {
    const p = local(e);
    const t = view.tf;
    if (e.pointerType === 'touch') {
      const prev = touches.get(e.pointerId);
      if (!prev) return;
      const others = [...touches.entries()].filter(([id]) => id !== e.pointerId).map(([, v]) => v);
      if (others.length === 0) {
        setTf({ ...t, tx: t.tx + (p[0] - prev.x), ty: t.ty + (p[1] - prev.y) });
      } else {
        // Two fingers: zoom about their midpoint, and follow it.
        const o = others[0];
        const d0 = Math.hypot(prev.x - o.x, prev.y - o.y) || 1;
        const d1 = Math.hypot(p[0] - o.x, p[1] - o.y) || 1;
        const mid0: Pt = [(prev.x + o.x) / 2, (prev.y + o.y) / 2];
        const mid1: Pt = [(p[0] + o.x) / 2, (p[1] + o.y) / 2];
        const k = Math.min(Math.max(d1 / d0, 0.5), 2);
        const scale = Math.min(2000, Math.max(0.5, t.scale * k));
        const f = scale / t.scale;
        setTf({ scale, tx: mid1[0] - (mid0[0] - t.tx) * f, ty: mid1[1] - (mid0[1] - t.ty) * f });
      }
      touches.set(e.pointerId, { x: p[0], y: p[1] });
      return;
    }
    if (panning && panning.id === e.pointerId) {
      setTf({ ...t, tx: t.tx + (p[0] - panning.x), ty: t.ty + (p[1] - panning.y) });
      panning.x = p[0];
      panning.y = p[1];
      return;
    }
    if (!drawing || drawing.id !== e.pointerId) return;
    if (e.pointerType === 'pen') penUntil = performance.now() + 700;
    const events = e.getCoalescedEvents?.() ?? [];
    for (const ev of events.length ? events : [e]) {
      const q = local(ev);
      if (drawing.erase) {
        eraseAt(q);
        continue;
      }
      if (!live) continue;
      const last = toScreen(view.tf, live.pts[live.pts.length - 1]);
      const gap = Math.hypot(q[0] - last[0], q[1] - last[1]);
      if (gap < 1.5) continue;
      drawing.moved += gap;
      live.pts.push(toWorld(view.tf, q));
      live.w.push(weight(ev));
    }
  }

  function up(e: PointerEvent) {
    touches.delete(e.pointerId);
    if (panning?.id === e.pointerId) panning = null;
    if (!drawing || drawing.id !== e.pointerId) return;
    const d = drawing;
    drawing = null;
    if (e.pointerType === 'pen') penUntil = performance.now() + 700;
    if (d.erase) {
      if (sketch.snapshot(pane) !== d.before) sketch.remember(pane, d.before);
      return;
    }
    const stroke = live;
    live = null;
    if (!stroke) return;
    if (d.moved < 3) {
      pickAt(local(e)); // a tap: pick the station under it instead of leaving a dot
      return;
    }
    const a = anchorStroke(stations, stroke.pts, stroke.kind, stroke.w.some((x) => x !== 1) ? stroke.w : undefined);
    if (!a) return;
    if (pane === 'section') {
      const at = sketch.selected;
      if (!at) return;
      sketch.strokes.section = { ...sketch.strokes.section, [at]: [...(sketch.strokes.section[at] ?? []), a] };
    } else sketch.strokes[pane] = [...sketch.strokes[pane], a];
    sketch.remember(pane, d.before);
  }

  /** Taps near a station pick it (the cross section then shows it, and the pad stops following the survey). */
  function pickAt(p: Pt) {
    if (pane === 'section') return;
    let best: string | null = null;
    let bd = 22;
    for (const s of stations) {
      const [x, y] = toScreen(view.tf, s.p);
      const dd = Math.hypot(x - p[0], y - p[1]);
      if (dd < bd) {
        bd = dd;
        best = s.label;
      }
    }
    if (best) {
      sketch.picked = best;
      sketch.followSurvey = false;
    }
  }

  function eraseAt(p: Pt) {
    const hit = (a: Anchored) => {
      const pts = placeStroke(stations, a);
      if (!pts) return false;
      const s = pts.map((q) => toScreen(view.tf, q));
      for (let i = 0; i < s.length; i++) {
        const [x0, y0] = s[i];
        const [x1, y1] = s[Math.min(i + 1, s.length - 1)];
        const dx = x1 - x0;
        const dy = y1 - y0;
        const len2 = dx * dx + dy * dy;
        const k = len2 === 0 ? 0 : Math.max(0, Math.min(1, ((p[0] - x0) * dx + (p[1] - y0) * dy) / len2));
        if (Math.hypot(p[0] - (x0 + k * dx), p[1] - (y0 + k * dy)) < 9) return true;
      }
      return false;
    };
    if (pane === 'section') {
      const at = sketch.selected;
      if (!at) return;
      const list = sketch.strokes.section[at] ?? [];
      const kept = list.filter((a) => !hit(a));
      if (kept.length !== list.length) sketch.strokes.section = { ...sketch.strokes.section, [at]: kept };
    } else {
      const kept = sketch.strokes[pane].filter((a) => !hit(a));
      if (kept.length !== sketch.strokes[pane].length) sketch.strokes[pane] = kept;
    }
  }

  function wheel(e: WheelEvent) {
    e.preventDefault();
    const p = local(e);
    const t = view.tf;
    const scale = Math.min(2000, Math.max(0.5, t.scale * Math.exp(-e.deltaY * 0.0015)));
    const f = scale / t.scale;
    setTf({ scale, tx: p[0] - (p[0] - t.tx) * f, ty: p[1] - (p[1] - t.ty) * f });
    free();
  }

  function onKey(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      sketch.restore(pane, e.shiftKey ? 'redo' : 'undo');
    }
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions: the pane takes Ctrl/Cmd+Z for its own undo -->
<section class="pane" aria-label={PANE_TITLE[pane]} tabindex="-1" onkeydown={onKey}>
  <header>
    {#if pane === 'section'}
      <button class="ic" title="Previous station" aria-label="Previous station" onclick={() => sketch.step(-1)}><ChevronLeft size={15} /></button>
      <span class="st">{sketch.selected ?? 'no station'}</span>
      <button class="ic" title="Next station" aria-label="Next station" onclick={() => sketch.step(1)}><ChevronRight size={15} /></button>
    {:else}
      <span class="hint">tap a station to see its section</span>
    {/if}
    <span class="sp"></span>
    <select value={selectValue} aria-label="{PANE_TITLE[pane]} scale" title="Drawing scale: inches of paper to feet of ground" onchange={(e) => chooseScale(e.currentTarget.value)}>
      <option value="fit">Fit survey</option>
      {#each SCALES as s}<option value={s.id}>{s.label}</option>{/each}
      {#if selectValue === 'custom'}<option value="custom" disabled>{describeScale(view.tf.scale)}</option>{/if}
    </select>
    <button class="ic" title="Undo in {PANE_TITLE[pane]}" aria-label="Undo in {PANE_TITLE[pane]}" disabled={!sketch.can(pane, 'undo')} onclick={() => sketch.restore(pane, 'undo')}><Undo2 size={15} /></button>
    <button class="ic" title="Redo in {PANE_TITLE[pane]}" aria-label="Redo in {PANE_TITLE[pane]}" disabled={!sketch.can(pane, 'redo')} onclick={() => sketch.restore(pane, 'redo')}><Redo2 size={15} /></button>
    <button class="ic" class:on={view.frame === 'auto' && view.scaleId === 'fit'} title="Fit the whole survey" aria-label="Fit {PANE_TITLE[pane]} to the survey" onclick={fitAll}><Maximize size={15} /></button>
  </header>
  <div class="canvas">
    <canvas
      use:measure
      aria-label="{PANE_TITLE[pane]} drawing surface"
      onpointerdown={down}
      onpointermove={move}
      onpointerup={up}
      onpointercancel={up}
      onwheel={wheel}
      ondblclick={reframe}
    ></canvas>
  </div>
</section>

<style>
  .pane { display: flex; flex-direction: column; height: 100%; min-height: 0; min-width: 0; background: var(--panel); color: var(--text); font-size: 12px; outline: none; }
  header { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 4px 6px; border-bottom: 1px solid var(--line); min-height: 34px; }
  header .hint { color: var(--muted); font-size: 11px; }
  header .st { min-width: 3.5em; text-align: center; font-weight: 600; }
  .sp { flex: 1; }
  select { font: inherit; font-size: 12px; padding: 4px 6px; min-height: 30px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); }
  button.ic { display: inline-flex; align-items: center; justify-content: center; width: 32px; height: 30px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); cursor: pointer; padding: 0; touch-action: manipulation; }
  button.ic:disabled { opacity: 0.4; cursor: default; }
  button.ic.on { border-color: var(--accent); color: var(--accent); }
  .canvas { flex: 1; min-height: 0; position: relative; }
  canvas { position: absolute; inset: 0; display: block; touch-action: none; cursor: crosshair; }
</style>
