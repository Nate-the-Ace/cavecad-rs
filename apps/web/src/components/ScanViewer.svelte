<script lang="ts">
  // Shows one scan big enough to read a station number: wheel (or pinch) zooms about the pointer, drag
  // pans, double-click fits, and the scan can be turned a quarter at a time (scans are often sideways).
  // This is only how it looks here; nothing about the view is stored. A PDF is handed to the browser's
  // own viewer.
  import { RotateCw, Maximize } from '@lucide/svelte';

  let {
    url,
    kind = 'image',
    name = '',
    marks = [],
    pending = null,
    picking = false,
    onpick,
    trim = null,
    draft = [],
    tracing = false,
    onstroke,
  }: {
    url: string;
    kind?: 'image' | 'pdf';
    name?: string;
    /** Spots already matched to stations, in scan pixels from the top left. */
    marks?: { px: number; py: number; label: string }[];
    /** A spot picked and waiting for its station. */
    pending?: { px: number; py: number } | null;
    /** A click on the scan picks a spot (instead of nothing). */
    picking?: boolean;
    onpick?: (px: number, py: number) => void;
    /** The saved outline the scan is trimmed to: what is outside is dimmed. */
    trim?: [number, number][] | null;
    /** An outline being traced, point by point. */
    draft?: [number, number][];
    /** A drag draws a line round the part to keep (a plain click still picks a point). */
    tracing?: boolean;
    onstroke?: (points: [number, number][]) => void;
  } = $props();

  let box = $state<HTMLDivElement>();
  let scale = $state(1);
  let x = $state(0);
  let y = $state(0);
  let turn = $state(0);
  let natural = $state({ w: 1, h: 1 });
  let downAt: { x: number; y: number } | null = null;
  let stroke = $state<[number, number][] | null>(null);
  let drawing = false;
  const pointers = new Map<number, { x: number; y: number }>();
  let pinch = 0;
  /** True until the person zooms or pans: then a resized panel keeps the scan fitted. */
  let fitted = true;

  /** Fits the whole scan in the box (the turned scan's size when it is on its side). */
  function fit() {
    if (!box) return;
    const sideways = turn % 2 === 1;
    const w = sideways ? natural.h : natural.w;
    const h = sideways ? natural.w : natural.h;
    scale = Math.min(box.clientWidth / w, box.clientHeight / h, 1.5) * 0.98;
    x = 0;
    y = 0;
    fitted = true;
  }

  function loaded(e: Event) {
    const img = e.currentTarget as HTMLImageElement;
    natural = { w: img.naturalWidth || 1, h: img.naturalHeight || 1 };
    turn = 0;
    fit();
  }

  /** Zooms by `k`, keeping the point under (cx, cy) (box coordinates from its centre) where it is. */
  function zoomAt(k: number, cx: number, cy: number) {
    const next = Math.min(Math.max(scale * k, 0.02), 40);
    const r = next / scale;
    x = cx - (cx - x) * r;
    y = cy - (cy - y) * r;
    scale = next;
    fitted = false;
  }

  function centred(e: { clientX: number; clientY: number }) {
    const r = box!.getBoundingClientRect();
    return { x: e.clientX - r.left - r.width / 2, y: e.clientY - r.top - r.height / 2 };
  }

  function wheel(e: WheelEvent) {
    e.preventDefault();
    const c = centred(e);
    zoomAt(Math.exp(-e.deltaY * (e.ctrlKey ? 0.01 : 0.0015)), c.x, c.y);
  }

  function down(e: PointerEvent) {
    box?.setPointerCapture(e.pointerId);
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
    downAt = pointers.size === 1 ? { x: e.clientX, y: e.clientY } : null;
    drawing = tracing && e.button === 0 && e.pointerType !== 'touch' ? pointers.size === 1 : tracing && e.pointerType === 'touch' && pointers.size === 1;
    if (pointers.size > 1) {
      drawing = false;
      stroke = null;
    }
    pinch = 0;
  }

  function move(e: PointerEvent) {
    const p = pointers.get(e.pointerId);
    if (!p) return;
    if (drawing && pointers.size === 1) {
      const q = pixelAt(e.clientX, e.clientY);
      const last = stroke?.[stroke.length - 1];
      if (!stroke && downAt && Math.hypot(e.clientX - downAt.x, e.clientY - downAt.y) >= 4) stroke = [[...Object.values(pixelAt(downAt.x, downAt.y))] as [number, number]];
      if (stroke && (!last || Math.hypot(q.px - last[0], q.py - last[1]) * scale >= 3)) stroke = [...stroke, [q.px, q.py]];
      pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
      return;
    }
    if (pointers.size === 2) {
      pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
      const [a, b] = [...pointers.values()];
      const d = Math.hypot(a.x - b.x, a.y - b.y);
      if (pinch) {
        const c = centred({ clientX: (a.x + b.x) / 2, clientY: (a.y + b.y) / 2 });
        zoomAt(d / pinch, c.x, c.y);
      }
      pinch = d;
      return;
    }
    x += e.clientX - p.x;
    y += e.clientY - p.y;
    fitted = false;
    pointers.set(e.pointerId, { x: e.clientX, y: e.clientY });
  }

  $effect(() => {
    if (!box) return;
    const ro = new ResizeObserver(() => fitted && fit());
    ro.observe(box);
    return () => ro.disconnect();
  });

  /** The scan pixel under a point on the screen (undoing the pan, zoom and turn). */
  function pixelAt(clientX: number, clientY: number): { px: number; py: number } {
    const c = centred({ clientX, clientY });
    const lx = (c.x - x) / scale;
    const ly = (c.y - y) / scale;
    const a = (-turn * Math.PI) / 2;
    const rx = lx * Math.cos(a) - ly * Math.sin(a);
    const ry = lx * Math.sin(a) + ly * Math.cos(a);
    return { px: rx + natural.w / 2, py: ry + natural.h / 2 };
  }

  const up = (e: PointerEvent) => {
    if (stroke) {
      if (stroke.length >= 8) onstroke?.(stroke);
      stroke = null;
      downAt = null;
      drawing = false;
      pointers.delete(e.pointerId);
      return;
    }
    drawing = false;
    // A press that did not move is a pick (when picking), not the end of a pan.
    if (picking && downAt && pointers.size === 1 && Math.hypot(e.clientX - downAt.x, e.clientY - downAt.y) < 4) {
      const p = pixelAt(e.clientX, e.clientY);
      if (p.px >= 0 && p.py >= 0 && p.px <= natural.w && p.py <= natural.h) onpick?.(p.px, p.py);
    }
    downAt = null;
    pointers.delete(e.pointerId);
    pinch = 0;
  };

  function key(e: KeyboardEvent) {
    if (e.key === '+' || e.key === '=') zoomAt(1.25, 0, 0);
    else if (e.key === '-') zoomAt(0.8, 0, 0);
    else if (e.key === '0') fit();
    else if (e.key.toLowerCase() === 'r') rotate();
  }

  function rotate() {
    turn = (turn + 1) % 4;
    fit();
  }
</script>

<div class="viewer">
  {#if kind === 'pdf'}
    <embed src={url} type="application/pdf" title={name} />
  {:else}
    <div class="tools">
      <button title="Fit the whole scan (0)" aria-label="Fit" onclick={fit}><Maximize size={14} /></button>
      <button title="Turn a quarter (R)" aria-label="Turn" onclick={rotate}><RotateCw size={14} /></button>
      <span class="zoom">{Math.round(scale * 100)}%</span>
    </div>
        <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions: the scan takes the wheel, drags and keys for zoom, pan, fit and turn -->
    <div bind:this={box} class="stage" class:picking={picking || tracing} role="application" aria-label={name} tabindex="0" onwheel={wheel} onpointerdown={down} onpointermove={move} onpointerup={up} onpointercancel={up} ondblclick={fit} onkeydown={key}>
      <div class="plane" style="width: {natural.w}px; height: {natural.h}px; transform: translate(-50%, -50%) translate({x}px, {y}px) scale({scale}) rotate({turn * 90}deg)">
        <img src={url} alt={name} draggable="false" onload={loaded} />
        <svg class="trim" width={natural.w} height={natural.h} viewBox="0 0 {natural.w} {natural.h}" aria-hidden="true">
          {#if trim}
            <path class="dim" fill-rule="evenodd" d="M0 0H{natural.w}V{natural.h}H0Z M{trim.map((p) => p.join(' ')).join(' L')}Z" />
            <polygon class="edge" points={trim.map((p) => p.join(',')).join(' ')} vector-effect="non-scaling-stroke" />
          {/if}
          {#if draft.length}
            <polyline class="draft" points={draft.map((p) => p.join(',')).join(' ')} vector-effect="non-scaling-stroke" />
            {#if draft.length > 1}<line class="closing" x1={draft[draft.length - 1][0]} y1={draft[draft.length - 1][1]} x2={draft[0][0]} y2={draft[0][1]} vector-effect="non-scaling-stroke" />{/if}
            {#each draft as p}<circle cx={p[0]} cy={p[1]} r={4 / scale} class="dot" />{/each}
          {/if}
          {#if stroke}<polyline class="draft" points={stroke.map((p) => p.join(',')).join(' ')} vector-effect="non-scaling-stroke" />{/if}
        </svg>
        {#each marks as m}
          <div class="pin" style="left: {m.px}px; top: {m.py}px"><span style="transform: scale({1 / scale}) rotate({-turn * 90}deg)"><i></i>{m.label}</span></div>
        {/each}
        {#if pending}
          <div class="pin wait" style="left: {pending.px}px; top: {pending.py}px"><span style="transform: scale({1 / scale}) rotate({-turn * 90}deg)"><i></i>?</span></div>
        {/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .viewer { position: relative; flex: 1; min-width: 0; min-height: 0; display: flex; background: var(--panel-2); }
  .stage { flex: 1; position: relative; overflow: hidden; cursor: grab; touch-action: none; outline: none; }
  .stage:active { cursor: grabbing; }
  .plane { position: absolute; left: 50%; top: 50%; transform-origin: center; }
  img { display: block; width: 100%; height: 100%; max-width: none; user-select: none; -webkit-user-drag: none; image-rendering: auto; }
  .stage.picking { cursor: crosshair; }
  .trim { position: absolute; left: 0; top: 0; pointer-events: none; overflow: visible; }
  .trim .dim { fill: rgba(0, 0, 0, 0.55); }
  .trim .edge { fill: none; stroke: #0a84ff; stroke-width: 2; stroke-dasharray: 8 5; }
  .trim .draft { fill: none; stroke: #0a84ff; stroke-width: 2; }
  .trim .closing { stroke: #0a84ff; stroke-width: 1; stroke-dasharray: 3 4; }
  .trim .dot { fill: #0a84ff; }
  .pin { position: absolute; width: 0; height: 0; }
  .pin span { position: absolute; left: 0; top: 0; transform-origin: 0 0; display: block; color: #ff4d2e; font: 600 12px sans-serif; white-space: nowrap; text-shadow: 0 0 3px #fff, 0 0 3px #fff; pointer-events: none; }
  .pin i { position: absolute; left: -7px; top: -7px; width: 14px; height: 14px; border-radius: 50%; border: 2px solid #ff4d2e; background: rgba(255, 77, 46, 0.2); }
  .pin span { padding: 6px 0 0 8px; }
  .pin.wait span, .pin.wait i { color: #0a84ff; border-color: #0a84ff; background: rgba(10, 132, 255, 0.2); }
  embed { flex: 1; border: 0; }
  .tools { position: absolute; top: 6px; right: 8px; z-index: 1; display: flex; align-items: center; gap: 4px; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); padding: 2px 4px; }
  .tools button { background: none; border: none; color: var(--text); cursor: pointer; padding: 3px; }
  .zoom { font-size: 11px; color: var(--muted); min-width: 3.2em; text-align: right; }
</style>
