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
  import { vf, pickFrame } from '../lib/viewframe.svelte';
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
  /** Paper inches (from the lower left) under a pointer. */
  function paperAt(e: PointerEvent | WheelEvent | MouseEvent): [number, number] {
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
  /** What a drag is doing to the picked frame: moving it, resizing it from a grip, or panning its map (focused inside). */
  let drag: { mode: 'move' | 'grip' | 'pan'; grip: number; x: number; y: number; rect: [number, number, number, number]; center: [number, number] } | null = null;
  let lockedTold = 0;

  /** The 8 grips of a frame (corners, then edge middles), paper inches. Index 0..3 corners: lower left, lower right, upper right, upper left. */
  function grips(r: [number, number, number, number]): [number, number][] {
    const [x0, y0, x1, y1] = r;
    const mx = (x0 + x1) / 2;
    const my = (y0 + y1) / 2;
    return [[x0, y0], [x1, y0], [x1, y1], [x0, y1], [mx, y0], [x1, my], [mx, y1], [x0, my]];
  }
  /** Paper inches moved by `d` turned into metres moved on the map, through the frame's scale and twist. */
  function paperToMap(f: { feet_per_inch: number; twist: number }, d: [number, number]): [number, number] {
    const m = f.feet_per_inch * 0.3048; // metres per inch of paper
    const t = (f.twist * Math.PI) / 180;
    return [(Math.cos(t) * d[0] + Math.sin(t) * d[1]) * m, (-Math.sin(t) * d[0] + Math.cos(t) * d[1]) * m];
  }
  function tellLocked() {
    if (Date.now() - lockedTold < 4000) return;
    lockedTold = Date.now();
    engine.log('info', 'sheet', 'This viewframe is locked: unlock it in the Viewframe tab to pan or zoom its map.');
  }
  /** The picked frame of the shown sheet (a live copy from `stored`). */
  function picked() {
    const def = stored.find((d) => d.id === chosen);
    const f = vf.sheet === chosen ? def?.frames.find((q) => q.id === vf.frame) : undefined;
    return def && f ? { def, f } : null;
  }
  function onDown(e: PointerEvent) {
    if (!chosen || e.button !== 0) return;
    const p = paperAt(e);
    const cur = picked();
    const near = 7 / shown.k; // grip reach, in inches
    if (cur && !vf.inside) {
      const gi = grips(cur.f.rect).findIndex((g) => Math.abs(g[0] - p[0]) <= near && Math.abs(g[1] - p[1]) <= near);
      if (gi >= 0) return begin(e, 'grip', gi, cur.f);
    }
    if (cur && vf.inside) {
      const r = cur.f.rect;
      if (p[0] >= r[0] && p[0] <= r[2] && p[1] >= r[1] && p[1] <= r[3]) {
        if (cur.f.locked) return tellLocked();
        return begin(e, 'pan', -1, cur.f);
      }
    }
    const hit = frameAt(p);
    if (!hit) {
      pickFrame('', '');
      paint();
      return;
    }
    pickFrame(chosen, hit.f.id);
    begin(e, 'move', -1, hit.f);
    paint();
  }
  function begin(e: PointerEvent, mode: 'move' | 'grip' | 'pan', grip: number, f: { rect: [number, number, number, number]; center: [number, number] }) {
    drag = { mode, grip, x: e.clientX, y: e.clientY, rect: [...f.rect] as [number, number, number, number], center: [...f.center] as [number, number] };
    canvas.setPointerCapture(e.pointerId);
  }
  function onMove(e: PointerEvent) {
    if (!drag) return;
    const cur = picked();
    if (!cur) return;
    const f = cur.f;
    const dx = (e.clientX - drag.x) / shown.k;
    const dy = -(e.clientY - drag.y) / shown.k;
    const r0 = drag.rect;
    if (drag.mode === 'pan') {
      const m = paperToMap(f, [dx, dy]);
      f.center = [drag.center[0] - m[0], drag.center[1] - m[1]];
    } else if (drag.mode === 'move') {
      f.rect = [r0[0] + dx, r0[1] + dy, r0[2] + dx, r0[3] + dy];
    } else {
      // resize from a grip; the map stays where it is on the paper (its centre follows the box's middle)
      let [x0, y0, x1, y1] = r0;
      const g = drag.grip;
      if (g === 0 || g === 3 || g === 7) x0 += dx;
      if (g === 1 || g === 2 || g === 5) x1 += dx;
      if (g === 0 || g === 1 || g === 4) y0 += dy;
      if (g === 2 || g === 3 || g === 6) y1 += dy;
      const min = 0.4;
      if (x1 - x0 < min) [x0, x1] = g === 0 || g === 3 || g === 7 ? [x1 - min, x1] : [x0, x0 + min];
      if (y1 - y0 < min) [y0, y1] = g === 0 || g === 1 || g === 4 ? [y1 - min, y1] : [y0, y0 + min];
      f.rect = [x0, y0, x1, y1];
      const shift: [number, number] = [(x0 + x1) / 2 - (r0[0] + r0[2]) / 2, (y0 + y1) / 2 - (r0[1] + r0[3]) / 2];
      const m = paperToMap(f, shift);
      f.center = [drag.center[0] + m[0], drag.center[1] + m[1]];
    }
    paintLive(cur.def);
  }
  function onUp() {
    if (!drag) return;
    const moved = drag;
    drag = null;
    const cur = picked();
    if (!cur) return;
    const same = moved.rect.every((v, i) => Math.abs(v - cur.f.rect[i]) < 1e-9) && Math.abs(moved.center[0] - cur.f.center[0]) < 1e-9 && Math.abs(moved.center[1] - cur.f.center[1]) < 1e-9;
    if (!same) {
      if (cur.def.auto) cur.def.auto.edited = true; // a hand-changed sheet is kept when the grid is made again
      const r = engine.sheetPut(cur.def);
      if (r !== 'ok') engine.log('warn', 'sheet', r);
    }
    readStored();
    paint();
  }
  /** Double-click inside a frame works inside it (its map has the focus); outside goes back to the paper. */
  function onDbl(e: MouseEvent) {
    if (!chosen) return;
    const hit = frameAt(paperAt(e));
    if (hit) {
      pickFrame(chosen, hit.f.id);
      vf.inside = true;
      if (hit.f.locked) tellLocked();
    } else vf.inside = false;
    paint();
  }
  /** The wheel zooms the focused frame's map about the pointer (not when locked). */
  function onWheel(e: WheelEvent) {
    const cur = picked();
    if (!chosen || !cur || !vf.inside) return;
    e.preventDefault();
    if (cur.f.locked) return tellLocked();
    const f = cur.f;
    const p = paperAt(e);
    const mid: [number, number] = [(f.rect[0] + f.rect[2]) / 2, (f.rect[1] + f.rect[3]) / 2];
    const before = paperToMap(f, [p[0] - mid[0], p[1] - mid[1]]);
    f.feet_per_inch = Math.max(0.5, f.feet_per_inch * (e.deltaY > 0 ? 1.15 : 1 / 1.15));
    const after = paperToMap(f, [p[0] - mid[0], p[1] - mid[1]]);
    f.center = [f.center[0] + before[0] - after[0], f.center[1] + before[1] - after[1]]; // the point under the pointer stays put
    const r = engine.sheetPut(cur.def);
    if (r !== 'ok') engine.log('warn', 'sheet', r);
    readStored();
    paint();
  }
  function onKey(e: KeyboardEvent) {
    if (e.key !== 'Escape') return;
    if (vf.inside) vf.inside = false;
    else pickFrame('', '');
    paint();
  }
  /** The picked frame's grips (or, focused inside, a heavy border), over the drawn sheet. */
  function drawPicked(g: CanvasRenderingContext2D, rects: Map<string, [number, number, number, number]>) {
    const r = vf.sheet === chosen ? rects.get(vf.frame) : undefined;
    if (!r) return;
    const P = (q: [number, number]): [number, number] => [q[0] * shown.k, (shown.h - q[1]) * shown.k];
    g.save();
    g.strokeStyle = '#188cff';
    if (vf.inside) {
      g.lineWidth = 3;
      const a = P([r[0], r[3]]);
      const b = P([r[2], r[1]]);
      g.strokeRect(a[0], a[1], b[0] - a[0], b[1] - a[1]);
    } else {
      g.fillStyle = '#188cff';
      for (const q of grips(r)) {
        const [x, y] = P(q);
        g.fillRect(x - 4, y - 4, 8, 8);
      }
    }
    g.restore();
  }
  /** Draws the sheet from a definition being dragged (not stored yet), so a drag is smooth and makes one undo step. */
  function paintLive(def: SheetDef) {
    const { scene, lines } = currentSheet();
    const r = engine.sheetRender(def.id);
    if (!r) return;
    for (const pf of r.frames) {
      const f = def.frames.find((q) => q.id === pf.id);
      if (!f) continue;
      pf.rect = [...f.rect] as [number, number, number, number];
      pf.clip = [];
      pf.center = [...f.center] as [number, number];
      pf.k = 1 / (f.feet_per_inch * 0.3048);
    }
    const g = canvas.getContext('2d');
    if (!g) return;
    const dpr = window.devicePixelRatio || 1;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    drawStoredSheet(g, r, scene, lines, shown.k, looks(), vf.frame);
    drawPicked(g, new Map(def.frames.map((f) => [f.id, f.rect])));
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
    readStored();
    pickFrame(chosen, id);
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
      drawStoredSheet(sg, rendered, scene, lines, k, looks(), vf.sheet === chosen ? vf.frame : '');
      drawPicked(sg, new Map(rendered.frames.map((f) => [f.id, f.rect])));
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
    void vf.frame;
    void vf.inside;
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

<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<div class="sheetpanel" bind:this={box} tabindex="0" role="application" aria-label="Sheet" onkeydown={onKey}>
  <div class="bar">
    <select bind:value={chosen} aria-label="Which sheet" title="The quick sheet of the Sheets tab, or a sheet of your own with viewframes">
      <option value="">Quick sheet</option>
      {#each stored as d (d.id)}<option value={d.id}>{d.name}</option>{/each}
    </select>
    <button onclick={newSheet} title="A new sheet of your own, kept in the cave, at the paper and scale set on the Sheets tab">New sheet</button>
    <button onclick={addViewframe} disabled={!chosen} title="Add a viewframe: a window onto the map at twice the detail. Click a frame to pick it (drag to move, grips to resize, scale in the Viewframe tab); double-click inside to pan and zoom its map, Esc to come out, in the sheet's free space beside the title block">Add viewframe</button>
  </div>
  <canvas bind:this={canvas} aria-label="The sheet" onpointerdown={onDown} onpointermove={onMove} onpointerup={onUp} onwheel={onWheel} ondblclick={onDbl}></canvas>
  <div class="note">{note}</div>
</div>

<style>
  .sheetpanel { position: relative; width: 100%; height: 100%; display: grid; place-items: center; background: var(--panel-2); overflow: hidden; }
  canvas { box-shadow: 0 2px 14px rgba(0, 0, 0, 0.35); }
  .bar { position: absolute; top: 8px; left: 8px; display: flex; gap: 6px; z-index: 2; }
  .bar select, .bar button { font-size: 12px; }
  .note { position: absolute; left: 12px; bottom: 8px; font-size: 12px; color: var(--muted); pointer-events: none; }
</style>
