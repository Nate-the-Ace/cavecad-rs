<script lang="ts">
  // Draws one motif: a small picture that repeats along a line or inside an area. Click points for straight strokes, drag for freehand
  // or for a circle. The drawing is centred and sized to 1 by the engine, so it can be drawn anywhere on the pad.
  import { changed } from '../../lib/cave.svelte';
  import { engine } from '../../lib/engine';
  import { drawMotif, plain, simplify, type Motif, type Prim } from '../../lib/patterns';
  import { onTheme } from '../../lib/theme';

  let { def, onsaved, onremoved, onduplicate }: { def: Motif; onsaved: (name: string) => void; onremoved: () => void; onduplicate: () => void } = $props();

  const PAD = 280;
  const GRID = 14;
  type Stroke = { kind: 'poly'; pts: [number, number][]; closed: boolean } | { kind: 'circle'; c: [number, number]; r: number };

  /** Unit coordinates (y up, longest side 1) to pad pixels, and back. */
  const toPad = (p: [number, number]): [number, number] => [PAD / 2 + p[0] * (PAD * 0.7), PAD / 2 - p[1] * (PAD * 0.7)];
  const fromPad = (p: [number, number]): [number, number] => [p[0], PAD - p[1]];

  function strokesOf(m: Motif): Stroke[] {
    return m.prims.map((p) =>
      p.t === 'circle' ? { kind: 'circle', c: toPad(p.c), r: p.r * PAD * 0.7 } : { kind: 'poly', pts: p.pts.map(toPad), closed: !!p.closed },
    );
  }

  let name = $state(def.name);
  let strokes = $state<Stroke[]>(strokesOf(def));
  let tool = $state<'line' | 'free' | 'circle'>('line');
  let snap = $state(true);
  let live = $state<Stroke | null>(null);
  let message = $state('');
  let canvas: HTMLCanvasElement;
  let theme = $state(0);
  const readonly = $derived(!def.own);

  const snapPt = (p: [number, number]): [number, number] => {
    if (!snap) return p;
    const s = PAD / GRID;
    return [Math.round(p[0] / s) * s, Math.round(p[1] / s) * s];
  };
  const at = (e: PointerEvent): [number, number] => {
    const r = canvas.getBoundingClientRect();
    return [((e.clientX - r.left) / r.width) * PAD, ((e.clientY - r.top) / r.height) * PAD];
  };

  function down(e: PointerEvent) {
    if (readonly) return;
    canvas.setPointerCapture(e.pointerId);
    const p = at(e);
    if (tool === 'circle') live = { kind: 'circle', c: snapPt(p), r: 0 };
    else if (tool === 'free') live = { kind: 'poly', pts: [p], closed: false };
    else if (live && live.kind === 'poly') live.pts.push(snapPt(p));
    else live = { kind: 'poly', pts: [snapPt(p), snapPt(p)], closed: false };
  }
  function move(e: PointerEvent) {
    if (!live) return;
    const p = at(e);
    if (live.kind === 'circle' && tool === 'circle' && e.buttons) live.r = Math.hypot(p[0] - live.c[0], p[1] - live.c[1]);
    else if (live.kind === 'poly' && tool === 'free' && e.buttons) live.pts.push(p);
    else if (live.kind === 'poly' && tool === 'line') live.pts[live.pts.length - 1] = snapPt(p);
  }
  function up() {
    if (!live) return;
    if (tool === 'circle') {
      if (live.kind === 'circle' && live.r > 2) strokes.push(live);
      live = null;
    } else if (tool === 'free') {
      if (live.kind === 'poly' && live.pts.length > 2) strokes.push({ kind: 'poly', pts: simplify(live.pts, 2.5), closed: false });
      live = null;
    }
  }
  /** Ends the stroke being clicked out: the points so far, without the one following the pointer. */
  function finish() {
    if (live && live.kind === 'poly' && tool === 'line') {
      const pts = live.pts.slice(0, -1);
      if (pts.length >= 2) strokes.push({ kind: 'poly', pts, closed: false });
      live = null;
    }
  }
  function close() {
    if (live && live.kind === 'poly' && tool === 'line') {
      const pts = live.pts.slice(0, -1);
      if (pts.length >= 3) strokes.push({ kind: 'poly', pts, closed: true });
      live = null;
    }
  }
  function key(e: KeyboardEvent) {
    if (e.key === 'Enter') finish();
    if (e.key === 'Escape') live = null;
  }

  const prims = $derived.by((): Prim[] =>
    strokes.map((s) =>
      s.kind === 'circle'
        ? { t: 'circle', c: fromPad(s.c), r: s.r }
        : { t: 'poly', pts: s.pts.map(fromPad), closed: s.closed },
    ),
  );
  const result = $derived(prims.length ? engine.motifFromDrawing(name, prims) : 'error: Draw something first.');
  const problem = $derived(typeof result === 'string' ? result.replace(/^error: /, '') : '');

  $effect(() => {
    const off = onTheme(() => theme++);
    return off;
  });
  $effect(() => {
    void theme;
    const all = live ? [...strokes, live] : strokes;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = PAD * dpr;
    canvas.height = PAD * dpr;
    const g = canvas.getContext('2d')!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, PAD, PAD);
    const ink = getComputedStyle(canvas).color;
    if (snap) {
      g.fillStyle = ink;
      g.globalAlpha = 0.25;
      for (let i = 0; i <= GRID; i++) for (let j = 0; j <= GRID; j++) g.fillRect((i * PAD) / GRID - 0.5, (j * PAD) / GRID - 0.5, 1, 1);
      g.globalAlpha = 1;
    }
    g.strokeStyle = ink;
    g.lineWidth = 1.6;
    g.lineJoin = 'round';
    g.lineCap = 'round';
    for (const s of all) {
      g.beginPath();
      if (s.kind === 'circle') g.arc(s.c[0], s.c[1], s.r, 0, Math.PI * 2);
      else {
        s.pts.forEach((p, i) => (i ? g.lineTo(p[0], p[1]) : g.moveTo(p[0], p[1])));
        if (s.closed) g.closePath();
      }
      g.stroke();
    }
  });

  let sample: HTMLCanvasElement;
  $effect(() => {
    void theme;
    const m = typeof result === 'string' ? null : result;
    const w = 260;
    const h = 40;
    const dpr = window.devicePixelRatio || 1;
    sample.width = w * dpr;
    sample.height = h * dpr;
    const g = sample.getContext('2d')!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, w, h);
    if (!m) return;
    g.strokeStyle = g.fillStyle = getComputedStyle(sample).color;
    g.lineWidth = 1;
    g.lineJoin = 'round';
    for (const [i, size] of [8, 14, 24, 32].entries()) drawMotif(g, m.prims, 30 + i * 62, h / 2, size);
  });

  function save() {
    if (typeof result === 'string') return;
    const r = engine.savePattern('motif', plain(result));
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    if (r === 'ok') {
      changed();
      onsaved(result.name);
    }
  }
  function remove() {
    const r = engine.removePattern('motif', def.name);
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    if (r === 'ok') {
      changed();
      onremoved();
    }
  }
</script>

<div class="editor">
  <label class="row">Name <input type="text" bind:value={name} maxlength="40" disabled={readonly} /></label>
  {#if readonly}
    <p class="note">This one comes with CaveCAD. <button onclick={onduplicate}>Duplicate to change it</button></p>
  {:else}
    <div class="row">
      <button class:on={tool === 'line'} onclick={() => { finish(); tool = 'line'; }}>Straight</button>
      <button class:on={tool === 'free'} onclick={() => { finish(); tool = 'free'; }}>Freehand</button>
      <button class:on={tool === 'circle'} onclick={() => { finish(); tool = 'circle'; }}>Circle</button>
      <label class="small"><input type="checkbox" bind:checked={snap} /> snap to grid</label>
    </div>
    <p class="hint">{tool === 'line' ? 'Click the corners. Double-click or Enter to finish; “Close shape” joins the end to the start.' : tool === 'free' ? 'Drag to draw; it is tidied into a few straight pieces.' : 'Drag from the middle outward.'}</p>
  {/if}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <canvas bind:this={canvas} style="width:{PAD}px;height:{PAD}px" tabindex="0" aria-label="Motif drawing pad" onpointerdown={down} onpointermove={move} onpointerup={up} ondblclick={finish} onkeydown={key}></canvas>
  <canvas bind:this={sample} style="width:260px;height:40px" class="sample" aria-label="The motif at four sizes"></canvas>
  {#if !readonly}
    <div class="row">
      <button onclick={() => strokes.pop()} disabled={!strokes.length}>Undo stroke</button>
      <button onclick={() => (strokes = [])} disabled={!strokes.length}>Clear</button>
      <button onclick={close} disabled={!live}>Close shape</button>
    </div>
  {/if}
  {#if problem && strokes.length}<p class="problem">{problem}</p>{/if}
  {#if message}<p class="problem">{message}</p>{/if}
  {#if !readonly}
    <div class="row">
      <button class="primary" onclick={save} disabled={typeof result === 'string'}>Save motif</button>
      <button onclick={remove}>Delete</button>
    </div>
  {/if}
</div>

<style>
  .editor { display: flex; flex-direction: column; gap: 8px; }
  .row { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  .small { font-size: 11px; color: var(--muted); display: flex; gap: 3px; align-items: center; }
  canvas { color: var(--text); background: var(--panel-2); border: 1px solid var(--line); border-radius: var(--radius); touch-action: none; cursor: crosshair; }
  canvas.sample { cursor: default; }
  input, select, button { font: inherit; padding: 3px 6px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); }
  input[type='text'] { flex: 1; min-width: 120px; }
  input[type='checkbox'] { padding: 0; }
  button { cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  button.on, .primary { background: var(--accent-bg); border-color: var(--accent); }
  .problem { color: var(--danger, #c0392b); font-size: 12px; margin: 0; }
  .note, .hint { font-size: 12px; color: var(--muted); margin: 0; }
</style>
