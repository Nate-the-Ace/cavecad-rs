<script lang="ts">
  // The sketchpad: the survey as it is taken, on three flat panes to draw on (plan, profile, cross
  // section). Pen draws, fingers pan and pinch (a pen stroke in progress ignores the palm), a mouse draws
  // and wheels to zoom. Strokes hang on the nearest station of their pane (lib/sketch-geometry.ts), so a
  // sketch stays attached as the survey grows. Every pane has its own undo and redo. The panes follow the
  // survey (fit to it, and the cross section shows the station being stood at) until you move one yourself.
  import { untrack } from 'svelte';
  import { ChevronLeft, ChevronRight, Maximize, Redo2, Undo2 } from '@lucide/svelte';
  import {
    anchorStroke, fitView, layout, placeStroke, planView, profileView, sectionView, toScreen, toWorld,
    type Anchored, type DimsByStation, type Pt, type SurveyShot, type Transform,
  } from '../lib/sketch-geometry';

  type Pane = 'plan' | 'profile' | 'section';
  type Tool = 'wall' | 'ledge' | 'note' | 'erase' | 'pan';
  interface Saved { plan: Anchored[]; profile: Anchored[]; section: Record<string, Anchored[]> }

  let { shots, dims, standing = null, storageKey = 'default' }: { shots: SurveyShot[]; dims: DimsByStation; standing?: string | null; storageKey?: string } = $props();

  const PANES: { id: Pane; title: string }[] = [
    { id: 'plan', title: 'Plan' },
    { id: 'profile', title: 'Profile' },
    { id: 'section', title: 'Cross section' },
  ];
  const TOOLS: { id: Tool; label: string; hint: string }[] = [
    { id: 'wall', label: 'Wall', hint: 'A solid line: passage wall' },
    { id: 'ledge', label: 'Ledge', hint: 'A dashed line: ledge or edge' },
    { id: 'note', label: 'Note', hint: 'A line in the accent colour: anything to remember' },
    { id: 'erase', label: 'Erase', hint: 'Drag over a line to remove it (the pen\'s eraser end does this too)' },
    { id: 'pan', label: 'Move', hint: 'Drag to move the view (two fingers always do)' },
  ];

  const L = $derived(layout(shots));
  const plan = $derived(planView(L, dims));
  const profile = $derived(profileView(L, dims));
  let picked = $state<string | null>(null);
  let followSurvey = $state(true);
  const selected = $derived(followSurvey && standing && L.stations[standing] ? standing : picked && L.stations[picked] ? picked : (L.chain[L.chain.length - 1] ?? null));
  const section = $derived(selected ? sectionView(L, dims, selected) : null);

  let tool = $state<Tool>('wall');
  let strokes = $state<Saved>(load());
  let tf = $state<Record<Pane, Transform>>({ plan: { scale: 20, tx: 100, ty: 100 }, profile: { scale: 20, tx: 100, ty: 100 }, section: { scale: 60, tx: 100, ty: 100 } });
  let fit = $state<Record<Pane, boolean>>({ plan: true, profile: true, section: true });
  let size = $state<Record<Pane, { w: number; h: number }>>({ plan: { w: 0, h: 0 }, profile: { w: 0, h: 0 }, section: { w: 0, h: 0 } });
  let live = $state<{ pane: Pane; pts: Pt[]; w: number[]; kind: string } | null>(null);
  let active = $state<Pane>('plan');
  let canvases: Record<Pane, HTMLCanvasElement | null> = { plan: null, profile: null, section: null };
  const history: Record<Pane, { undo: string[]; redo: string[] }> = $state({ plan: { undo: [], redo: [] }, profile: { undo: [], redo: [] }, section: { undo: [], redo: [] } });

  // ---- storage ----
  function load(): Saved {
    try {
      const raw = JSON.parse(localStorage.getItem(`cavecad.sketch.${storageKey}`) ?? 'null');
      if (raw && Array.isArray(raw.plan) && Array.isArray(raw.profile) && raw.section && typeof raw.section === 'object') return raw as Saved;
    } catch {
      /* nothing kept, or storage unavailable */
    }
    return { plan: [], profile: [], section: {} };
  }
  function save() {
    try {
      localStorage.setItem(`cavecad.sketch.${storageKey}`, JSON.stringify($state.snapshot(strokes)));
    } catch {
      /* the sketch just is not kept */
    }
  }

  // ---- what each pane shows, in its own metres ----
  const stationsOf = (pane: Pane): { label: string; p: Pt }[] => (pane === 'plan' ? plan.stations : pane === 'profile' ? profile.stations : selected ? [{ label: selected, p: [0, 0] }] : []);
  function fitPoints(pane: Pane): Pt[] {
    if (pane === 'plan') return [...plan.stations.map((s) => s.p), ...plan.splays.map((s) => s[1]), ...plan.walls.flat()];
    if (pane === 'profile') return [...profile.stations.map((s) => s.p), ...profile.walls.flat()];
    const s = section;
    return s ? [[0, 0], ...(s.outline ?? []), ...s.splays] : [];
  }
  const listFor = (pane: Pane): Anchored[] => (pane === 'section' ? (selected ? strokes.section[selected] ?? [] : []) : strokes[pane]);

  // Panes follow the survey until the person moves one.
  $effect(() => {
    void L;
    void plan;
    void profile;
    void section;
    for (const pane of ['plan', 'profile', 'section'] as Pane[]) {
      const { w, h } = size[pane];
      if (!fit[pane] || w === 0) continue;
      untrack(() => (tf[pane] = fitView(fitPoints(pane), w, h, 30, pane === 'section' ? 140 : 90)));
    }
  });

  // ---- drawing ----
  let raf = 0;
  function redraw() {
    cancelAnimationFrame(raf);
    raf = requestAnimationFrame(() => (['plan', 'profile', 'section'] as Pane[]).forEach(draw));
  }
  $effect(() => {
    void L; void plan; void profile; void section; void strokes; void tf; void live; void selected; void standing; void tool; void size;
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

  function draw(pane: Pane) {
    const cv = canvases[pane];
    const { w, h } = size[pane];
    if (!cv || w === 0) return;
    const ctx = cv.getContext('2d');
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const c = { text: css(cv, '--text', '#ddd'), muted: css(cv, '--muted', '#888'), accent: css(cv, '--accent', '#4af'), warn: css(cv, '--warn', '#fb3'), line: css(cv, '--line', '#444') };
    const t = tf[pane];
    ctx.font = '11px system-ui, sans-serif';

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
      const v = pane === 'plan' ? plan : profile;
      ctx.fillStyle = c.muted + '26';
      for (const poly_ of v.passage) {
        poly(ctx, t, poly_, true);
        ctx.fill();
      }
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
        for (const [a, b] of plan.splays) {
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
      const here = v.stations.find((s) => s.label === standing);
      if (here) ring(here.p, c.warn, 9);
      const sel = v.stations.find((s) => s.label === selected);
      if (sel && selected !== standing) ring(sel.p, c.accent, 8);
      if (pane === 'plan') {
        // north arrow
        ctx.fillStyle = c.muted;
        ctx.fillText('N', w - 22, 18);
        ctx.strokeStyle = c.muted;
        ctx.beginPath();
        ctx.moveTo(w - 18, 40);
        ctx.lineTo(w - 18, 24);
        ctx.lineTo(w - 22, 30);
        ctx.moveTo(w - 18, 24);
        ctx.lineTo(w - 14, 30);
        ctx.stroke();
      }
      // scale bar
      const nice = [0.5, 1, 2, 5, 10, 20, 50, 100].find((m) => m * t.scale >= 50) ?? 100;
      ctx.strokeStyle = c.muted;
      ctx.fillStyle = c.muted;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.moveTo(12, h - 14);
      ctx.lineTo(12 + nice * t.scale, h - 14);
      ctx.stroke();
      ctx.fillText(`${nice} m`, 12, h - 20);
    } else if (section) {
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
        ctx.fillStyle = c.muted + '26';
        ctx.strokeStyle = c.text;
        ctx.lineWidth = 1.4;
        poly(ctx, t, section.outline, true);
        ctx.fill();
        ctx.stroke();
        const [l, u, r, d] = section.outline;
        ctx.fillStyle = c.muted;
        const val = (n: number) => `${+Math.abs(n).toFixed(2)}`;
        ctx.fillText(`L ${val(l[0])}`, toScreen(t, l)[0] + 4, toScreen(t, l)[1] - 6);
        ctx.fillText(`U ${val(u[1])}`, toScreen(t, u)[0] + 6, toScreen(t, u)[1] + 12);
        ctx.fillText(`R ${val(r[0])}`, toScreen(t, r)[0] - 34, toScreen(t, r)[1] - 6);
        ctx.fillText(`D ${val(d[1])}`, toScreen(t, d)[0] + 6, toScreen(t, d)[1] - 4);
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
    const stations = stationsOf(pane);
    for (const a of listFor(pane)) {
      const pts = placeStroke(stations, a);
      if (pts) ink(ctx, t, pts, a.w, a.kind, c);
    }
    if (live && live.pane === pane) ink(ctx, t, live.pts, live.w, live.kind, c);
  }

  // ---- the canvases' size ----
  function measure(node: HTMLCanvasElement, pane: Pane) {
    canvases[pane] = node;
    const apply = () => {
      const r = node.parentElement!.getBoundingClientRect();
      const dpr = window.devicePixelRatio || 1;
      const w = Math.max(0, Math.floor(r.width));
      const h = Math.max(0, Math.floor(r.height));
      node.width = Math.max(1, Math.floor(w * dpr));
      node.height = Math.max(1, Math.floor(h * dpr));
      node.style.width = `${w}px`;
      node.style.height = `${h}px`;
      size[pane] = { w, h };
    };
    const ro = new ResizeObserver(apply);
    ro.observe(node.parentElement!);
    apply();
    return { destroy: () => ro.disconnect() };
  }

  // ---- history ----
  const snapshot = (pane: Pane) => JSON.stringify($state.snapshot(pane === 'section' ? strokes.section : strokes[pane]));
  function remember(pane: Pane, before: string) {
    history[pane].undo.push(before);
    if (history[pane].undo.length > 100) history[pane].undo.shift();
    history[pane].redo = [];
    save();
  }
  function restore(pane: Pane, from: 'undo' | 'redo') {
    const stack = history[pane][from];
    const text = stack.pop();
    if (text === undefined) return;
    history[pane][from === 'undo' ? 'redo' : 'undo'].push(snapshot(pane));
    const value = JSON.parse(text);
    if (pane === 'section') strokes.section = value;
    else strokes[pane] = value;
    save();
  }

  // ---- pointer input ----
  interface Touch { x: number; y: number }
  const touches = new Map<number, Touch>();
  let penUntil = 0;
  let drawing: { pane: Pane; id: number; before: string; erase: boolean; moved: number } | null = null;
  let panning: { pane: Pane; id: number; x: number; y: number } | null = null;

  const local = (e: PointerEvent, pane: Pane): Pt => {
    const r = canvases[pane]!.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  };
  const stop = (pane: Pane) => (fit[pane] = false);

  function down(e: PointerEvent, pane: Pane) {
    active = pane;
    const cv = canvases[pane]!;
    try {
      cv.setPointerCapture(e.pointerId);
    } catch {
      /* the pointer is already gone: carry on without capture */
    }
    const p = local(e, pane);
    if (e.pointerType === 'touch') {
      // Fingers move the view. A pen stroke in progress (or just finished) means this is a palm.
      if (drawing || performance.now() < penUntil) return;
      touches.set(e.pointerId, { x: p[0], y: p[1] });
      stop(pane);
      return;
    }
    if (e.pointerType === 'pen') penUntil = performance.now() + 700;
    const eraserEnd = e.pointerType === 'pen' && (e.buttons & 32) !== 0;
    if (e.button === 1 || (tool === 'pan' && !eraserEnd)) {
      panning = { pane, id: e.pointerId, x: p[0], y: p[1] };
      stop(pane);
      return;
    }
    if (e.button !== 0 && e.pointerType === 'mouse') return;
    const erase = tool === 'erase' || eraserEnd;
    drawing = { pane, id: e.pointerId, before: snapshot(pane), erase, moved: 0 };
    if (!erase) live = { pane, pts: [toWorld(tf[pane], p)], w: [weight(e)], kind: tool === 'note' ? 'note' : tool === 'ledge' ? 'ledge' : 'wall' };
    else eraseAt(pane, p);
  }

  const weight = (e: PointerEvent) => (e.pointerType === 'pen' ? Math.min(1.8, 0.45 + e.pressure * 1.2) : 1);

  function move(e: PointerEvent, pane: Pane) {
    const p = local(e, pane);
    if (e.pointerType === 'touch') {
      const prev = touches.get(e.pointerId);
      if (!prev) return;
      const others = [...touches.entries()].filter(([id]) => id !== e.pointerId).map(([, v]) => v);
      const t = tf[pane];
      if (others.length === 0) {
        tf[pane] = { ...t, tx: t.tx + (p[0] - prev.x), ty: t.ty + (p[1] - prev.y) };
      } else {
        // Two fingers: zoom about their midpoint, and follow it.
        const o = others[0];
        const d0 = Math.hypot(prev.x - o.x, prev.y - o.y) || 1;
        const d1 = Math.hypot(p[0] - o.x, p[1] - o.y) || 1;
        const mid0: Pt = [(prev.x + o.x) / 2, (prev.y + o.y) / 2];
        const mid1: Pt = [(p[0] + o.x) / 2, (p[1] + o.y) / 2];
        const k = Math.min(Math.max(d1 / d0, 0.5), 2);
        const scale = Math.min(400, Math.max(1, t.scale * k));
        const f = scale / t.scale;
        tf[pane] = { scale, tx: mid1[0] - (mid0[0] - t.tx) * f, ty: mid1[1] - (mid0[1] - t.ty) * f };
      }
      touches.set(e.pointerId, { x: p[0], y: p[1] });
      return;
    }
    if (panning && panning.id === e.pointerId) {
      const t = tf[panning.pane];
      tf[panning.pane] = { ...t, tx: t.tx + (p[0] - panning.x), ty: t.ty + (p[1] - panning.y) };
      panning.x = p[0];
      panning.y = p[1];
      return;
    }
    if (!drawing || drawing.id !== e.pointerId) return;
    if (e.pointerType === 'pen') penUntil = performance.now() + 700;
    const events = e.getCoalescedEvents?.() ?? [];
    for (const ev of events.length ? events : [e]) {
      const q = local(ev, pane);
      if (drawing.erase) {
        eraseAt(pane, q);
        continue;
      }
      if (!live) continue;
      const last = toScreen(tf[pane], live.pts[live.pts.length - 1]);
      const gap = Math.hypot(q[0] - last[0], q[1] - last[1]);
      if (gap < 1.5) continue;
      drawing.moved += gap;
      live.pts.push(toWorld(tf[pane], q));
      live.w.push(weight(ev));
    }
  }

  function up(e: PointerEvent, pane: Pane) {
    touches.delete(e.pointerId);
    if (panning?.id === e.pointerId) panning = null;
    if (!drawing || drawing.id !== e.pointerId) return;
    const d = drawing;
    drawing = null;
    if (e.pointerType === 'pen') penUntil = performance.now() + 700;
    if (d.erase) {
      if (snapshot(pane) !== d.before) remember(pane, d.before);
      return;
    }
    const stroke = live;
    live = null;
    if (!stroke) return;
    if (d.moved < 3) {
      // A tap: pick the station under it instead of leaving a dot.
      pickAt(pane, local(e, pane));
      return;
    }
    const a = anchorStroke(stationsOf(pane), stroke.pts, stroke.kind, stroke.w.some((x) => x !== 1) ? stroke.w : undefined);
    if (!a) return;
    if (pane === 'section') {
      if (!selected) return;
      strokes.section = { ...strokes.section, [selected]: [...(strokes.section[selected] ?? []), a] };
    } else strokes[pane] = [...strokes[pane], a];
    remember(pane, d.before);
  }

  /** Taps near a station pick it (the cross section then shows it, and the pad stops following the survey). */
  function pickAt(pane: Pane, p: Pt) {
    if (pane === 'section') return;
    let best: string | null = null;
    let bd = 22;
    for (const s of stationsOf(pane)) {
      const [x, y] = toScreen(tf[pane], s.p);
      const dd = Math.hypot(x - p[0], y - p[1]);
      if (dd < bd) {
        bd = dd;
        best = s.label;
      }
    }
    if (best) {
      picked = best;
      followSurvey = false;
    }
  }

  function eraseAt(pane: Pane, p: Pt) {
    const stations = stationsOf(pane);
    const hit = (a: Anchored) => {
      const pts = placeStroke(stations, a);
      if (!pts) return false;
      const s = pts.map((q) => toScreen(tf[pane], q));
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
      if (!selected) return;
      const list = strokes.section[selected] ?? [];
      const kept = list.filter((a) => !hit(a));
      if (kept.length !== list.length) strokes.section = { ...strokes.section, [selected]: kept };
    } else {
      const kept = strokes[pane].filter((a) => !hit(a));
      if (kept.length !== strokes[pane].length) strokes[pane] = kept;
    }
  }

  function wheel(e: WheelEvent, pane: Pane) {
    e.preventDefault();
    const p = local(e as unknown as PointerEvent, pane);
    const t = tf[pane];
    const scale = Math.min(400, Math.max(1, t.scale * Math.exp(-e.deltaY * 0.0015)));
    const f = scale / t.scale;
    tf[pane] = { scale, tx: p[0] - (p[0] - t.tx) * f, ty: p[1] - (p[1] - t.ty) * f };
    stop(pane);
  }

  function refit(pane: Pane) {
    fit[pane] = true;
    const { w, h } = size[pane];
    if (w) tf[pane] = fitView(fitPoints(pane), w, h, 30, pane === 'section' ? 140 : 90);
  }

  /** Steps the cross section to the previous or next station along the chain. */
  function step(by: number) {
    const chain = L.chain;
    if (chain.length === 0) return;
    const i = Math.max(0, chain.indexOf(selected ?? ''));
    picked = chain[Math.min(chain.length - 1, Math.max(0, i + by))];
    followSurvey = false;
  }

  function onKey(e: KeyboardEvent) {
    if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'z') {
      e.preventDefault();
      restore(active, e.shiftKey ? 'redo' : 'undo');
    }
  }

  const can = (pane: Pane, which: 'undo' | 'redo') => history[pane][which].length > 0;
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions: the pad takes Ctrl/Cmd+Z for the pane last drawn on -->
<div class="pad" role="region" aria-label="Sketchpad" tabindex="-1" onkeydown={onKey}>
  <div class="tools" role="toolbar" aria-label="Sketch tools">
    {#each TOOLS as t}
      <button class:sel={tool === t.id} aria-pressed={tool === t.id} title={t.hint} onclick={() => (tool = t.id)}>{t.label}</button>
    {/each}
    <span class="sp"></span>
    <label class="chk" title="Show the station you are standing at in the cross section, and keep the panes fitted to the survey"><input type="checkbox" bind:checked={followSurvey} /> Follow the survey</label>
  </div>
  <div class="grid">
    {#each PANES as pane}
      <section class="pane {pane.id}" aria-label={pane.title}>
        <header>
          <b>{pane.title}</b>
          {#if pane.id === 'section'}
            <button class="ic" title="Previous station" aria-label="Previous station" onclick={() => step(-1)}><ChevronLeft size={15} /></button>
            <span class="st">{selected ?? 'no station'}</span>
            <button class="ic" title="Next station" aria-label="Next station" onclick={() => step(1)}><ChevronRight size={15} /></button>
          {:else}
            <span class="hint">tap a station</span>
          {/if}
          <span class="sp"></span>
          <button class="ic" title="Undo in {pane.title}" aria-label="Undo in {pane.title}" disabled={!can(pane.id, 'undo')} onclick={() => restore(pane.id, 'undo')}><Undo2 size={15} /></button>
          <button class="ic" title="Redo in {pane.title}" aria-label="Redo in {pane.title}" disabled={!can(pane.id, 'redo')} onclick={() => restore(pane.id, 'redo')}><Redo2 size={15} /></button>
          <button class="ic" class:on={fit[pane.id]} title="Fit to the survey" aria-label="Fit {pane.title} to the survey" onclick={() => refit(pane.id)}><Maximize size={15} /></button>
        </header>
        <div class="canvas">
          <canvas
            use:measure={pane.id}
            aria-label="{pane.title} drawing surface"
            onpointerdown={(e) => down(e, pane.id)}
            onpointermove={(e) => move(e, pane.id)}
            onpointerup={(e) => up(e, pane.id)}
            onpointercancel={(e) => up(e, pane.id)}
            onwheel={(e) => wheel(e, pane.id)}
            ondblclick={() => refit(pane.id)}
          ></canvas>
        </div>
      </section>
    {/each}
  </div>
</div>

<style>
  .pad { display: flex; flex-direction: column; height: 100%; min-height: 0; background: var(--panel); color: var(--text); font-size: 13px; outline: none; }
  .tools { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 6px 8px; border-bottom: 1px solid var(--line); }
  .tools button { font: inherit; padding: 8px 14px; min-height: 40px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); cursor: pointer; touch-action: manipulation; }
  .tools button.sel { background: var(--accent); border-color: var(--accent); color: var(--on-accent, #fff); }
  .sp { flex: 1; }
  .chk { display: inline-flex; align-items: center; gap: 6px; color: var(--muted); min-height: 40px; }
  .chk input { width: 18px; height: 18px; }
  .grid { flex: 1; min-height: 0; display: grid; grid-template-columns: 3fr 2fr; grid-template-rows: 1fr 1fr; gap: 6px; padding: 6px; }
  .pane { display: flex; flex-direction: column; min-height: 0; min-width: 0; border: 1px solid var(--line); border-radius: var(--radius); overflow: hidden; background: var(--panel-2); }
  .pane.plan { grid-row: 1 / span 2; }
  header { display: flex; align-items: center; gap: 6px; padding: 3px 6px; border-bottom: 1px solid var(--line); font-size: 12px; min-height: 30px; }
  header .hint { color: var(--muted); font-size: 11px; }
  header .st { min-width: 3.5em; text-align: center; font-weight: 600; }
  button.ic { display: inline-flex; align-items: center; justify-content: center; width: 30px; height: 28px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); color: var(--text); cursor: pointer; padding: 0; touch-action: manipulation; }
  button.ic:disabled { opacity: 0.4; cursor: default; }
  button.ic.on { border-color: var(--accent); color: var(--accent); }
  .canvas { flex: 1; min-height: 0; position: relative; }
  canvas { position: absolute; inset: 0; display: block; touch-action: none; cursor: crosshair; }
</style>
