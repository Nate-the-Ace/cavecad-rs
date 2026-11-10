<script lang="ts">
  // The sheet: the cave's map on a piece of paper at a plot scale, with a border, title block, scale bar
  // and north arrow. What is drawn here is what the Print and PNG buttons in the Sheets tab produce.
  import { onMount, untrack } from 'svelte';
  import { onTheme } from '../lib/theme';
  import { cave, option } from '../lib/cave.svelte';
  import { engine, loadEngine } from '../lib/engine';
  import { looks } from '../lib/ink';
  import { followUnits, scaleText, sheet } from '../lib/sheet.svelte';
  import { drawSheet, drawStoredSheet, overflows } from '../lib/sheet-draw';
  import type { SheetDef } from '../lib/engine';
  import { currentSheet, shownSheet } from '../lib/sheet-export';

  let canvas: HTMLCanvasElement;
  let box: HTMLDivElement;
  let ready = $state(false);
  let note = $state('');
  let size = $state({ w: 600, h: 400 });
  /** The stored sheets of this cave (each with its own viewframes), and the one shown ('' = the quick sheet of the Sheets tab). */
  let stored = $state<SheetDef[]>([]);
  let chosen = $state('');
  /** Pixels per inch and paper height of the stored sheet as last drawn (to turn a pointer into paper inches). */
  let shown = { k: 1, h: 0 };
  /** A frame being dragged: moved on the paper, or (Shift) its map slid inside it. */
  let drag: { id: string; pan: boolean; x: number; y: number; rect: [number, number, number, number]; center: [number, number] } | null = null;

  /** Paper inches (from the lower left) under a pointer. */
  function paperAt(e: PointerEvent | WheelEvent): [number, number] {
    const r = canvas.getBoundingClientRect();
    return [(e.clientX - r.left) / shown.k, shown.h - (e.clientY - r.top) / shown.k];
  }
  /** The topmost viewframe under a paper point, from the stored sheet. */
  function frameAt(p: [number, number]) {
    const def = stored.find((d) => d.id === chosen);
    if (!def) return null;
    for (let i = def.frames.length - 1; i >= 0; i--) {
      const f = def.frames[i];
      if (p[0] >= f.rect[0] && p[0] <= f.rect[2] && p[1] >= f.rect[1] && p[1] <= f.rect[3]) return { def, f };
    }
    return null;
  }
  function onDown(e: PointerEvent) {
    if (!chosen || e.button !== 0) return;
    const hit = frameAt(paperAt(e));
    if (!hit) return;
    if (hit.f.locked) return engine.log('info', 'sheet', 'That viewframe is locked.');
    chosenFrame = hit.f.id;
    drag = { id: hit.f.id, pan: e.shiftKey, x: e.clientX, y: e.clientY, rect: [...hit.f.rect] as [number, number, number, number], center: [...hit.f.center] as [number, number] };
    canvas.setPointerCapture(e.pointerId);
  }
  function onMove(e: PointerEvent) {
    if (!drag) return;
    const def = stored.find((d) => d.id === chosen);
    const f = def?.frames.find((q) => q.id === drag!.id);
    if (!f) return;
    const dx = (e.clientX - drag.x) / shown.k;
    const dy = -(e.clientY - drag.y) / shown.k;
    if (drag.pan) {
      // slide the map: paper inches back to metres through the frame's scale and turn
      const kIn = 1 / (f.feet_per_inch * 0.3048); // inches of paper per metre
      const t = (f.twist * Math.PI) / 180;
      const mx = (Math.cos(t) * dx + Math.sin(t) * dy) / kIn;
      const my = (-Math.sin(t) * dx + Math.cos(t) * dy) / kIn;
      f.center = [drag.center[0] - mx, drag.center[1] - my];
    } else f.rect = [drag.rect[0] + dx, drag.rect[1] + dy, drag.rect[2] + dx, drag.rect[3] + dy];
    if (def) paintLive(def);
  }
  function onUp() {
    if (!drag) return;
    drag = null;
    const def = stored.find((d) => d.id === chosen);
    if (def) {
      if (def.auto) def.auto.edited = true; // a hand-moved sheet is kept when the grid is made again
      const r = engine.sheetPut(def);
      if (r !== 'ok') engine.log('warn', 'sheet', r);
    }
    readStored();
    paint();
  }
  /** The wheel over a frame zooms its map: up for more detail, down for less. */
  function onWheel(e: WheelEvent) {
    if (!chosen) return;
    const hit = frameAt(paperAt(e));
    if (!hit || hit.f.locked) return;
    e.preventDefault();
    hit.f.feet_per_inch = Math.max(1, hit.f.feet_per_inch * (e.deltaY > 0 ? 1.15 : 1 / 1.15));
    chosenFrame = hit.f.id;
    const r = engine.sheetPut(hit.def);
    if (r !== 'ok') engine.log('warn', 'sheet', r);
    readStored();
    paint();
  }
  /** Draws the sheet from a definition being dragged (not stored yet), so a drag is smooth and makes one undo step. */
  function paintLive(def: SheetDef) {
    const { scene, lines } = currentSheet();
    const r = engine.sheetRender(def.id);
    if (!r) return;
    // the stored render, with the dragged frame's live place swapped in
    for (const pf of r.frames) {
      const f = def.frames.find((q) => q.id === pf.id);
      if (!f) continue;
      const shift: [number, number] = [f.rect[0] - pf.rect[0], f.rect[1] - pf.rect[1]];
      pf.rect = [...f.rect] as [number, number, number, number];
      pf.clip = pf.clip.map((q) => [q[0] + shift[0], q[1] + shift[1]] as [number, number]);
      pf.center = [...f.center] as [number, number];
    }
    const g = canvas.getContext('2d');
    if (!g) return;
    const dpr = window.devicePixelRatio || 1;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    drawStoredSheet(g, r, scene, lines, shown.k, looks(), chosenFrame);
  }

  /** The viewframe last added or picked, outlined in the highlight colour. */
  let chosenFrame = $state('');

  function readStored() {
    try {
      stored = engine.sheetsList();
    } catch {
      stored = [];
    }
    if (chosen && !stored.some((d) => d.id === chosen)) chosen = '';
  }

  /** A new stored sheet over the middle of the cave, at the quick sheet's paper and scale. */
  function newSheet() {
    const { plan, scene } = currentSheet();
    const b = scene.bounds;
    const cx = b ? (b[0] + b[3]) / 2 : 0;
    const cy = b ? (b[1] + b[4]) / 2 : 0;
    const papers = engine.sheetOptions().papers;
    const paper = papers.find((p) => Math.abs(p.w - Math.min(plan.w, plan.h)) < 0.01 && Math.abs(p.h - Math.max(plan.w, plan.h)) < 0.01)?.name ?? papers[0]?.name ?? '';
    const r = engine.sheetNew(`Sheet ${stored.length + 1}`, paper, plan.w > plan.h, cx, cy, plan.feet_per_inch || 50);
    if (r.startsWith('error')) return engine.log('warn', 'sheet', r);
    // the main frame sits above the title band, so it is smaller than the quick sheet's map: pick the most detailed
    // standard scale at which the whole cave fits inside it
    const def = engine.sheetsList().find((d) => d.id === r);
    const main = def?.frames[0];
    if (def && main && b) {
      const ftX = ((b[3] - b[0]) / 0.3048) * 1.08;
      const ftY = ((b[4] - b[1]) / 0.3048) * 1.08;
      const need = Math.max(ftX / (main.rect[2] - main.rect[0]), ftY / (main.rect[3] - main.rect[1]));
      const scales = engine.sheetOptions().scales.map((sc) => sc.feet_per_inch).sort((x, y) => x - y);
      main.feet_per_inch = scales.find((f) => f >= need) ?? Math.ceil(need);
      const put = engine.sheetPut(def);
      if (put !== 'ok') engine.log('warn', 'sheet', put);
    }
    readStored();
    chosen = r;
    chosenFrame = '';
    paint();
  }

  /** Adds a viewframe to the shown sheet: a window in its free space beside the title block, looking at the same place at twice the detail. */
  function addViewframe() {
    const def = stored.find((d) => d.id === chosen);
    const r = chosen ? engine.sheetRender(chosen) : null;
    if (!def || !r) return;
    const base = def.frames[0];
    const id = `frame-${def.frames.length + 1}-${Math.random().toString(36).slice(2, 7)}`;
    const w = r.w;
    const h = r.h;
    // aim it at the station nearest the main frame's middle, so it shows passage, not rock
    const { scene } = currentSheet();
    const from = base ? base.center : [0, 0];
    let aim: [number, number] = [from[0], from[1]];
    let best = Infinity;
    for (const st of scene.stations) {
      const d = (st.at[0] - from[0]) ** 2 + (st.at[1] - from[1]) ** 2;
      if (d < best) {
        best = d;
        aim = [st.at[0], st.at[1]];
      }
    }
    def.frames.push({ id, rect: [w * 0.55, h * 0.14, w * 0.96, h * 0.56], center: aim, feet_per_inch: base ? base.feet_per_inch / 2 : 25, twist: 0, locked: false, hidden: [], clip: [], label: `Detail ${def.frames.length}` });
    const res = engine.sheetPut(def);
    if (res !== 'ok') return engine.log('warn', 'sheet', res);
    chosenFrame = id;
    readStored();
    paint();
  }
  loadEngine().then(() => (ready = true));

  function paint() {
    if (!ready || !canvas) return;
    const { plan, scene, lines } = currentSheet();
    const rendered = chosen ? engine.sheetRender(chosen) : null;
    if (rendered) {
      // a stored sheet: the map through each of its viewframes
      const pad = 16;
      const k = Math.max(5, Math.min((size.w - pad * 2) / rendered.w, (size.h - pad * 2) / rendered.h));
      const dpr = window.devicePixelRatio || 1;
      canvas.width = Math.round(rendered.w * k * dpr);
      canvas.height = Math.round(rendered.h * k * dpr);
      canvas.style.width = `${rendered.w * k}px`;
      canvas.style.height = `${rendered.h * k}px`;
      const sg = canvas.getContext('2d');
      if (!sg) return;
      sg.setTransform(dpr, 0, 0, dpr, 0, 0);
      shown = { k, h: rendered.h };
      drawStoredSheet(sg, rendered, scene, lines, k, looks(), chosenFrame);
      note = rendered.warnings.join(' ') || `${rendered.frames.length} viewframe${rendered.frames.length === 1 ? '' : 's'}`;
      return;
    }
    // the whole sheet, as large as the panel allows
    const pad = 16;
    const k = Math.max(5, Math.min((size.w - pad * 2) / plan.w, (size.h - pad * 2) / plan.h));
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(plan.w * k * dpr);
    canvas.height = Math.round(plan.h * k * dpr);
    canvas.style.width = `${plan.w * k}px`;
    canvas.style.height = `${plan.h * k}px`;
    const g = canvas.getContext('2d');
    if (!g) return;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    drawSheet(g, plan, scene, lines, k, looks());
    const per = plan.feet_per_inch;
    note = overflows(plan, scene.bounds) ? 'The cave does not fit this paper at this scale: pick a bigger sheet or a smaller scale.' : sheet.feetPerInch === 0 && !plan.fits ? 'No scale fits this cave on this paper.' : `Plotted at ${scaleText(per, cave.units)}`;
  }

  $effect(() => {
    void cave.version;
    void cave.adjusted;
    void cave.fileName;
    void cave.units;
    void ready;
    void size;
    void sheet.paper;
    void sheet.turned;
    void sheet.feetPerInch;
    void sheet.name;
    void sheet.lines;
    void sheet.title;
    void sheet.bar;
    void sheet.north;
    void option('handInk', false); // Ink style redraws every line
    shownSheet.id = chosen; // Print and PNG save the sheet shown here
    void chosenFrame;
    untrack(() => {
      followUnits(cave.units); // the paper of the units, until one is picked
      readStored();
      paint();
    });
  });

  onMount(() => {
    const ro = new ResizeObserver(() => (size = { w: box.clientWidth, h: box.clientHeight }));
    ro.observe(box);
    const off = onTheme(() => untrack(paint));
    return () => {
      ro.disconnect();
      off?.();
    };
  });
  void engine;
</script>

<div class="sheetpanel" bind:this={box}>
  <div class="bar">
    <select bind:value={chosen} aria-label="Which sheet" title="The quick sheet of the Sheets tab, or a sheet of your own with viewframes">
      <option value="">Quick sheet</option>
      {#each stored as d (d.id)}<option value={d.id}>{d.name}</option>{/each}
    </select>
    <button onclick={newSheet} title="A new sheet of your own, kept in the cave, at the paper and scale set on the Sheets tab">New sheet</button>
    <button onclick={addViewframe} disabled={!chosen} title="Add a viewframe: a window onto the map at twice the detail. Drag a frame to move it, Shift+drag to slide its map, scroll over it to zoom its map, in the sheet's free space beside the title block">Add viewframe</button>
  </div>
  <canvas bind:this={canvas} aria-label="The sheet" onpointerdown={onDown} onpointermove={onMove} onpointerup={onUp} onwheel={onWheel}></canvas>
  <div class="note">{note}</div>
</div>

<style>
  .sheetpanel { position: relative; width: 100%; height: 100%; display: grid; place-items: center; background: var(--panel-2); overflow: hidden; }
  canvas { box-shadow: 0 2px 14px rgba(0, 0, 0, 0.35); }
  .bar { position: absolute; top: 8px; left: 8px; display: flex; gap: 6px; z-index: 2; }
  .bar select, .bar button { font-size: 12px; }
  .note { position: absolute; left: 12px; bottom: 8px; font-size: 12px; color: var(--muted); pointer-events: none; }
</style>
