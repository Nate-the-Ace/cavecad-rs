<script lang="ts">
  // Draws one motif: a small picture that repeats along a line or inside an area. Click corners for straight strokes, drag for
  // freehand or for a circle. The drawing is centred and sized to 1 by the engine, so it can be drawn anywhere on the pad. Under
  // the pad it is shown at four sizes and repeated along a line, the way a linetype would use it.
  import { changed } from '../../lib/cave.svelte';
  import { engine } from '../../lib/engine';
  import { drawMotif, plain, simplify, type Motif, type Prim } from '../../lib/patterns';
  import { onTheme } from '../../lib/theme';

  let {
    def,
    onsaved,
    onremoved,
    onduplicate,
    ondirty,
    unsaved = false,
    fromLibrary = false,
  }: {
    def: Motif;
    onsaved: (name: string) => void;
    onremoved: () => void;
    onduplicate: () => void;
    ondirty?: (dirty: boolean) => void;
    /** A new motif or a copy not saved yet: Save is on from the start. */
    unsaved?: boolean;
    /** Shown from the account library, not in this cave yet: the banner offers Use in this cave. */
    fromLibrary?: boolean;
  } = $props();

  const PAD = 280;
  const GRID = 14;
  type Stroke = { kind: 'poly'; pts: [number, number][]; closed: boolean; fill: boolean } | { kind: 'circle'; c: [number, number]; r: number; fill: boolean };

  /** Unit coordinates (y up, longest side 1) to pad pixels. */
  const toPad = (p: [number, number]): [number, number] => [PAD / 2 + p[0] * (PAD * 0.7), PAD / 2 - p[1] * (PAD * 0.7)];
  const fromPad = (p: [number, number]): [number, number] => [p[0], PAD - p[1]];

  function strokesOf(m: Motif): Stroke[] {
    return m.prims.map((p) =>
      p.t === 'circle'
        ? { kind: 'circle', c: toPad(p.c), r: p.r * PAD * 0.7, fill: !!p.fill }
        : { kind: 'poly', pts: p.pts.map(toPad), closed: !!p.closed, fill: !!p.fill },
    );
  }

  let name = $state(def.name);
  let strokes = $state<Stroke[]>(strokesOf(def));
  let tool = $state<'line' | 'free' | 'circle'>('line');
  let snap = $state(true);
  /** Closed shapes and circles drawn from now on are filled in solid. */
  let solid = $state(false);
  let live = $state<Stroke | null>(null);
  let message = $state('');
  let canvas: HTMLCanvasElement;
  let theme = $state(0);
  /** Something was drawn, taken back or renamed since it was opened. */
  let edited = $state(false);
  const readonly = $derived(!def.own);
  const dirty = $derived(!readonly && (unsaved || edited));
  $effect(() => ondirty?.(dirty));
  $effect(() => {
    if (name !== def.name) edited = true;
  });

  const snapPt = (p: [number, number]): [number, number] => {
    if (!snap) return p;
    const s = PAD / GRID;
    return [Math.round(p[0] / s) * s, Math.round(p[1] / s) * s];
  };
  const at = (e: PointerEvent): [number, number] => {
    const r = canvas.getBoundingClientRect();
    return [((e.clientX - r.left) / r.width) * PAD, ((e.clientY - r.top) / r.height) * PAD];
  };
  function keep(s: Stroke) {
    strokes.push(s);
    edited = true;
  }

  function down(e: PointerEvent) {
    if (readonly) return;
    canvas.setPointerCapture(e.pointerId);
    canvas.focus();
    const p = at(e);
    edited = true;
    if (tool === 'circle') live = { kind: 'circle', c: snapPt(p), r: 0, fill: solid };
    else if (tool === 'free') live = { kind: 'poly', pts: [p], closed: false, fill: false };
    else if (live && live.kind === 'poly') live.pts.push(snapPt(p));
    else live = { kind: 'poly', pts: [snapPt(p), snapPt(p)], closed: false, fill: false };
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
      if (live.kind === 'circle' && live.r > 2) keep(live);
      live = null;
    } else if (tool === 'free') {
      // each drag is its own line, ended when you let go
      lastFree = -1;
      if (live.kind === 'poly' && live.pts.length > 2) {
        keep({ kind: 'poly', pts: simplify(live.pts, 2.5), closed: false, fill: false });
        lastFree = strokes.length - 1;
      }
      live = null;
    }
  }
  /** The freehand line just drawn (its index), which Close shape can still close; -1 when there is none. */
  let lastFree = $state(-1);
  /** The points of the open line: clicked corners without the one following the pointer, or a freehand trace tidied. */
  function openPoints(): [number, number][] {
    if (!live || live.kind !== 'poly') return [];
    return tool === 'line' ? live.pts.slice(0, -1) : simplify(live.pts, 2.5);
  }
  /** Ends the open line where it is. */
  function finish() {
    if (live && live.kind === 'poly') {
      const pts = openPoints();
      if (pts.length >= 2) keep({ kind: 'poly', pts, closed: false, fill: false });
      live = null;
    }
  }
  /** Ends the open line by joining it back to its start; in freehand, closes the line just drawn. */
  function close() {
    const last = strokes[lastFree];
    if (!live && tool === 'free' && last && last.kind === 'poly' && !last.closed && last.pts.length >= 3) {
      strokes[lastFree] = { ...last, closed: true, fill: solid };
      lastFree = -1;
      edited = true;
      return;
    }
    if (live && live.kind === 'poly') {
      const pts = openPoints();
      if (pts.length >= 3) keep({ kind: 'poly', pts, closed: true, fill: solid });
      live = null;
    }
  }
  /** A line is open (being clicked out or traced) and can be finished or closed. */
  const lineOpen = $derived(!!live && live.kind === 'poly');
  /** Close shape can act: on an open line, or on the freehand line just drawn. */
  const canClose = $derived.by(() => {
    if (lineOpen) return true;
    const last = tool === 'free' ? strokes[lastFree] : undefined;
    return !!last && last.kind === 'poly' && !last.closed && last.pts.length >= 3;
  });
  function key(e: KeyboardEvent) {
    if (e.key === 'Enter') finish();
    else if (e.key === 'Escape') live = null;
    else if ((e.key === 'z' && (e.metaKey || e.ctrlKey)) || e.key === 'Backspace') undoStroke();
    else return;
    e.preventDefault();
  }
  function undoStroke() {
    if (live) live = null;
    else if (strokes.length) {
      strokes.pop();
      lastFree = -1;
      edited = true;
    }
  }
  function clearAll() {
    if (!strokes.length || !confirm('Clear the whole drawing?')) return;
    strokes = [];
    live = null;
    edited = true;
  }
  function pickTool(t: typeof tool) {
    finish();
    lastFree = -1;
    tool = t;
  }

  const prims = $derived.by((): Prim[] =>
    strokes.map((s) =>
      s.kind === 'circle'
        ? { t: 'circle', c: fromPad(s.c), r: s.r, fill: s.fill }
        : { t: 'poly', pts: s.pts.map(fromPad), closed: s.closed, fill: s.closed && s.fill },
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
    g.fillStyle = ink;
    if (snap) {
      g.globalAlpha = 0.25;
      for (let i = 0; i <= GRID; i++) for (let j = 0; j <= GRID; j++) g.fillRect((i * PAD) / GRID - 0.5, (j * PAD) / GRID - 0.5, 1, 1);
      g.globalAlpha = 1;
    }
    // the middle line: the line a linetype stands the motif on, running left to right
    g.save();
    g.globalAlpha = 0.35;
    g.setLineDash([4, 4]);
    g.strokeStyle = ink;
    g.beginPath();
    g.moveTo(0, PAD / 2);
    g.lineTo(PAD, PAD / 2);
    g.stroke();
    g.restore();
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
      if (s.fill && (s.kind === 'circle' || s.closed)) g.fill();
      g.stroke();
    }
  });

  let sample: HTMLCanvasElement;
  $effect(() => {
    void theme;
    const m = typeof result === 'string' ? null : result;
    const w = 280;
    const h = 92;
    const dpr = window.devicePixelRatio || 1;
    sample.width = w * dpr;
    sample.height = h * dpr;
    const g = sample.getContext('2d')!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, w, h);
    if (!m) return;
    const ink = getComputedStyle(sample).color;
    g.strokeStyle = g.fillStyle = ink;
    g.lineWidth = 1;
    g.lineJoin = 'round';
    // four sizes
    for (const [i, size] of [8, 14, 22, 30].entries()) drawMotif(g, m.prims, 30 + i * 66, 24, size);
    // along a line, as a linetype would stand it
    const y = 70;
    g.globalAlpha = 0.5;
    g.beginPath();
    g.moveTo(8, y);
    g.lineTo(w - 8, y);
    g.stroke();
    g.globalAlpha = 1;
    for (let x = 26; x < w - 10; x += 34) drawMotif(g, m.prims, x, y, 14);
  });

  function save() {
    finish(); // a line still open is part of the drawing
    if (typeof result === 'string') return;
    const r = engine.savePattern('motif', plain(result));
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    if (r === 'ok') {
      changed();
      edited = false;
      onsaved(result.name);
    }
  }
  function remove() {
    if (!confirm(`Delete the motif “${def.name}”?`)) return;
    const r = engine.removePattern('motif', def.name);
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    if (r === 'ok') {
      changed();
      onremoved();
    }
  }
  function revert() {
    name = def.name;
    strokes = strokesOf(def);
    live = null;
    edited = false;
  }

  const HINT = {
    line: 'Click each corner. Press Finish line (or Enter, or double-click) to end it, or Close shape to join it back to the start.',
    free: 'Drag to draw a line; it ends when you let go and is tidied into a few straight pieces. Each drag is a separate line. Close shape joins the line you just drew back to its start.',
    circle: 'Press at the middle and drag outward.',
  };
</script>

<div class="editor">
  {#if readonly}
    <div class="builtin">
      {#if fromLibrary}
        <span><b>{def.name}</b> is in your library. Put it in this cave to use it or change it.</span>
        <button class="primary" onclick={onduplicate}>Use in this cave</button>
      {:else}
        <span><b>{def.name}</b> comes with CaveCAD and can’t be changed. Make your own copy to change it.</span>
        <button class="primary" onclick={onduplicate}>Make my own copy</button>
      {/if}
    </div>
  {:else}
    <label class="row name">Name <input type="text" bind:value={name} maxlength="40" /></label>
    <div class="row">
      <span class="seg" role="radiogroup" aria-label="Drawing tool">
        <button role="radio" aria-checked={tool === 'line'} class:on={tool === 'line'} onclick={() => pickTool('line')}>Straight</button>
        <button role="radio" aria-checked={tool === 'free'} class:on={tool === 'free'} onclick={() => pickTool('free')}>Freehand</button>
        <button role="radio" aria-checked={tool === 'circle'} class:on={tool === 'circle'} onclick={() => pickTool('circle')}>Circle</button>
      </span>
      <label class="check" title="Closed shapes and circles you draw from now on are filled in"><input type="checkbox" bind:checked={solid} /> Solid</label>
      <label class="check"><input type="checkbox" bind:checked={snap} /> Snap to grid</label>
    </div>
    <p class="hint">{HINT[tool]}</p>
  {/if}
  <div class="padrow">
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <canvas bind:this={canvas} style="width:{PAD}px;height:{PAD}px" tabindex="0" aria-label="Motif drawing pad" onpointerdown={down} onpointermove={move} onpointerup={up} ondblclick={finish} onkeydown={key}></canvas>
    {#if !readonly}
      <div class="side">
        {#if tool === 'line'}<button onclick={finish} disabled={!lineOpen} title="End the line here (Enter)">Finish line</button>{/if}
        <button onclick={close} disabled={!canClose} title={tool === 'free' ? 'Join the end of the line you just drew back to its start' : 'Join the end back to the start'}>Close shape</button>
        <button onclick={undoStroke} disabled={!live && !strokes.length} title="Take back the last stroke (Backspace)">Undo stroke</button>
        <button onclick={clearAll} disabled={!strokes.length}>Clear…</button>
        <p class="muted">The dashed line is the line a linetype stands the motif on.</p>
      </div>
    {/if}
  </div>
  <div class="samples">
    <span class="muted">At four sizes, and along a line:</span>
    <canvas bind:this={sample} style="width:280px;height:92px" class="sample" aria-label="The motif at four sizes and along a line"></canvas>
  </div>

  <div class="footer">
    {#if problem && strokes.length}<p class="problem">{problem}</p>{/if}
    {#if message}<p class="problem">{message}</p>{/if}
    {#if !readonly}
      <div class="row actions">
        <button class="primary" onclick={save} disabled={(typeof result === 'string' && !lineOpen) || !dirty}>Save</button>
        <button onclick={revert} disabled={!dirty || unsaved} title="Go back to how it was last saved">Undo changes</button>
        {#if dirty}<span class="unsaved">Not saved yet</span>{/if}
        {#if !unsaved}<button class="danger" onclick={remove}>Delete…</button>{/if}
      </div>
      {#if !dirty}<p class="muted">Use it in a linetype or a fill: add a Motif part there and pick “{def.name}”.</p>{/if}
    {/if}
  </div>
</div>

<style>
  .editor { display: flex; flex-direction: column; gap: 8px; }
  .row { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; }
  .name input { flex: 1; min-width: 120px; }
  .builtin { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; padding: 8px; border: 1px solid var(--accent); border-radius: var(--radius); background: var(--accent-bg); font-size: 12px; }
  .builtin span { flex: 1; min-width: 180px; }
  .seg { display: inline-flex; border: 1px solid var(--line); border-radius: var(--radius); overflow: hidden; }
  .seg button { border: none; border-radius: 0; padding: 3px 10px; background: var(--panel-2); color: var(--muted); }
  .seg button.on { background: var(--accent-bg); color: var(--text); }
  .check { font-size: 12px; color: var(--muted); display: flex; gap: 4px; align-items: center; }
  .padrow { display: flex; gap: 10px; align-items: flex-start; flex-wrap: wrap; }
  .side { display: flex; flex-direction: column; gap: 5px; width: 130px; }
  .side p { margin: 4px 0 0; }
  .samples { display: flex; flex-direction: column; gap: 3px; }
  canvas { color: var(--text); background: var(--panel-2); border: 1px solid var(--line); border-radius: var(--radius); touch-action: none; cursor: crosshair; }
  canvas.sample { cursor: default; }
  input, select, button { font: inherit; padding: 3px 6px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); }
  input[type='checkbox'] { padding: 0; }
  button { cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  .primary { background: var(--accent-bg); border-color: var(--accent); }
  .muted { color: var(--muted); font-size: 11px; }
  .hint { font-size: 12px; color: var(--muted); margin: 0; }
  .actions .danger { margin-left: auto; color: var(--danger, #c0392b); }
  .unsaved { font-size: 11px; color: var(--warn, #d08a00); }
  .problem { color: var(--danger, #c0392b); font-size: 12px; margin: 0; }
  .footer { position: sticky; bottom: 0; display: flex; flex-direction: column; gap: 6px; padding: 8px 0 2px; background: var(--panel); border-top: 1px solid var(--line); }
  .footer p { margin: 0; }
</style>
