<script lang="ts">
  import { onMount } from 'svelte';
  import { DEMOS } from '../lib/demos';
  import { engine, loadEngine, type Scene } from '../lib/engine';

  let canvas: HTMLCanvasElement;
  let wrap: HTMLDivElement;
  let scene = $state<Scene | null>(null);
  let demo = $state('');
  let adjusted = $state(true);
  let status = $state('');

  // View: world metres -> screen pixels. `scale` is pixels per metre; the origin is the screen
  // position of world (0, 0). Plan is drawn north-up, so world y is flipped.
  let scale = 1;
  let ox = 0;
  let oy = 0;
  let width = $state(0);
  let height = $state(0);
  let frame = 0;

  const colors = () => {
    const css = getComputedStyle(document.documentElement);
    const v = (n: string) => css.getPropertyValue(n).trim();
    return { bg: v('--panel-2'), text: v('--text'), muted: v('--muted'), accent: v('--accent'), danger: v('--danger'), warn: v('--warn'), line: v('--line') };
  };

  const toScreen = (p: [number, number]): [number, number] => [ox + p[0] * scale, oy - p[1] * scale];

  function fit() {
    if (!scene?.bounds || width === 0) return;
    const [x0, y0, x1, y1] = scene.bounds;
    const w = Math.max(x1 - x0, 1e-6);
    const h = Math.max(y1 - y0, 1e-6);
    const pad = 40;
    scale = Math.min((width - 2 * pad) / w, (height - 2 * pad) / h);
    ox = (width - scale * (x0 + x1)) / 2;
    oy = (height + scale * (y0 + y1)) / 2;
    schedule();
  }

  function schedule() {
    if (!frame) frame = requestAnimationFrame(() => ((frame = 0), draw()));
  }

  function niceLength(target: number): number {
    const p = Math.pow(10, Math.floor(Math.log10(target)));
    const m = target / p;
    return (m >= 5 ? 5 : m >= 2 ? 2 : 1) * p;
  }

  function draw() {
    if (!canvas) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    const g = canvas.getContext('2d')!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const c = colors();
    g.fillStyle = c.bg;
    g.fillRect(0, 0, width, height);
    if (!scene || scene.stations.length === 0) {
      g.fillStyle = c.muted;
      g.font = '14px system-ui, sans-serif';
      g.textAlign = 'center';
      g.fillText('Pick a demo cave above to see the map.', width / 2, height / 2);
      return;
    }

    g.lineCap = 'round';
    g.strokeStyle = c.muted;
    g.globalAlpha = 0.45;
    g.lineWidth = 1;
    g.beginPath();
    for (const [x1, y1, x2, y2] of scene.splays) {
      const a = toScreen([x1, y1]);
      const b = toScreen([x2, y2]);
      g.moveTo(a[0], a[1]);
      g.lineTo(b[0], b[1]);
    }
    g.stroke();
    g.globalAlpha = 1;

    for (const style of ['new', 'tie', 'closure'] as const) {
      g.strokeStyle = style === 'new' ? c.text : style === 'tie' ? c.warn : c.accent;
      g.lineWidth = style === 'new' ? 1.6 : 1.4;
      g.setLineDash(style === 'new' ? [] : [6, 4]);
      g.beginPath();
      for (const l of scene.legs) {
        if (l.style !== style) continue;
        const a = toScreen(l.from);
        const b = toScreen(l.to);
        g.moveTo(a[0], a[1]);
        g.lineTo(b[0], b[1]);
      }
      g.stroke();
    }
    g.setLineDash([]);

    const showLabels = scale > 3;
    g.font = '11px system-ui, sans-serif';
    g.textAlign = 'left';
    for (const s of scene.stations) {
      const [x, y] = toScreen(s.at);
      if (x < -20 || y < -20 || x > width + 20 || y > height + 20) continue;
      g.fillStyle = s.anchor ? c.accent : c.text;
      g.beginPath();
      g.arc(x, y, s.anchor ? 3.5 : 2, 0, Math.PI * 2);
      g.fill();
      if (showLabels) {
        g.fillStyle = c.muted;
        g.fillText(s.label, x + 5, y - 4);
      }
    }

    for (const k of scene.closures) {
      const [x, y] = toScreen(k.at);
      g.strokeStyle = c.danger;
      g.lineWidth = 1.5;
      g.beginPath();
      g.arc(x, y, 7, 0, Math.PI * 2);
      g.stroke();
      if (k.percent !== null && scale > 1.2) {
        g.fillStyle = c.danger;
        g.fillText(`${k.percent.toFixed(2)}%`, x + 10, y + 4);
      }
    }

    // scale bar
    const metres = niceLength(110 / scale);
    const len = metres * scale;
    g.strokeStyle = c.text;
    g.fillStyle = c.text;
    g.lineWidth = 2;
    g.beginPath();
    g.moveTo(16, height - 18);
    g.lineTo(16 + len, height - 18);
    g.stroke();
    g.textAlign = 'left';
    g.fillText(`${metres} m`, 16, height - 24);
  }

  function load(id: string) {
    const d = DEMOS.find((x) => x.id === id);
    if (!d) return;
    const result = engine.openDemo(d.json);
    status = result;
    refresh();
    fit();
  }

  function refresh() {
    scene = engine.scene(adjusted);
    schedule();
  }

  let drag: { x: number; y: number } | null = null;
  const onDown = (e: PointerEvent) => {
    drag = { x: e.clientX, y: e.clientY };
    canvas.setPointerCapture(e.pointerId);
  };
  const onMove = (e: PointerEvent) => {
    if (!drag) return;
    ox += e.clientX - drag.x;
    oy += e.clientY - drag.y;
    drag = { x: e.clientX, y: e.clientY };
    schedule();
  };
  const onUp = () => (drag = null);
  const onWheel = (e: WheelEvent) => {
    e.preventDefault();
    const rect = canvas.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    const my = e.clientY - rect.top;
    const f = Math.exp(-e.deltaY * 0.0015);
    ox = mx - (mx - ox) * f;
    oy = my - (my - oy) * f;
    scale *= f;
    schedule();
  };

  onMount(() => {
    const ro = new ResizeObserver(() => {
      width = wrap.clientWidth;
      height = wrap.clientHeight;
      if (scene && ox === 0 && oy === 0) fit();
      schedule();
    });
    ro.observe(wrap);
    const dark = window.matchMedia('(prefers-color-scheme: dark)');
    dark.addEventListener('change', schedule);
    loadEngine().then(() => schedule());
    return () => {
      ro.disconnect();
      dark.removeEventListener('change', schedule);
    };
  });
</script>

<div class="map" bind:this={wrap}>
  <canvas bind:this={canvas} style="width:{width}px;height:{height}px" onpointerdown={onDown} onpointermove={onMove} onpointerup={onUp} onwheel={onWheel}></canvas>
  <div class="bar">
    <select bind:value={demo} onchange={() => load(demo)} aria-label="Demo cave">
      <option value="" disabled>Open a demo cave…</option>
      {#each DEMOS as d}<option value={d.id}>{d.title}</option>{/each}
    </select>
    <label><input type="checkbox" bind:checked={adjusted} onchange={refresh} /> Adjusted</label>
    <button onclick={fit}>Fit</button>
    {#if status}<span class="status">{status}</span>{/if}
  </div>
</div>

<style>
  .map { position: relative; height: 100%; overflow: hidden; background: var(--panel-2); }
  canvas { display: block; touch-action: none; cursor: grab; }
  canvas:active { cursor: grabbing; }
  .bar { position: absolute; top: 8px; left: 8px; display: flex; gap: 8px; align-items: center; padding: 6px 8px; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); font-size: 12px; }
  select, button { font: inherit; padding: 3px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); color: var(--text); }
  label { display: flex; gap: 4px; align-items: center; color: var(--muted); }
  .status { color: var(--muted); }
</style>
