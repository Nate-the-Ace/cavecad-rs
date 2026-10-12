<script lang="ts">
  // The sheet: the cave's map on a piece of paper at a plot scale, with a border, title block, scale bar
  // and north arrow. What is drawn here is what the Print and PNG buttons in the Sheets tab produce.
  import { onMount, untrack } from 'svelte';
  import { onTheme } from '../lib/theme';
  import { cave } from '../lib/cave.svelte';
  import { engine, loadEngine } from '../lib/engine';
  import { looks } from '../lib/ink';
  import { sheet } from '../lib/sheet.svelte';
  import { drawSheet, overflows } from '../lib/sheet-draw';
  import { currentSheet } from '../lib/sheet-export';

  let canvas: HTMLCanvasElement;
  let box: HTMLDivElement;
  let ready = $state(false);
  let note = $state('');
  let size = $state({ w: 600, h: 400 });
  loadEngine().then(() => (ready = true));

  function paint() {
    if (!ready || !canvas) return;
    const { plan, scene, lines } = currentSheet();
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
    note = overflows(plan, scene.bounds) ? 'The cave does not fit this paper at this scale: pick a bigger sheet or a smaller scale.' : sheet.feetPerInch === 0 && !plan.fits ? 'No scale fits this cave on this paper.' : `Plotted at ${per < 10 ? per.toFixed(2) : per.toFixed(0)} ft per inch`;
  }

  $effect(() => {
    void cave.version;
    void cave.adjusted;
    void cave.fileName;
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
    untrack(paint);
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
  <canvas bind:this={canvas} aria-label="The sheet"></canvas>
  <div class="note">{note}</div>
</div>

<style>
  .sheetpanel { position: relative; width: 100%; height: 100%; display: grid; place-items: center; background: var(--panel-2); overflow: hidden; }
  canvas { box-shadow: 0 2px 14px rgba(0, 0, 0, 0.35); }
  .note { position: absolute; left: 12px; bottom: 8px; font-size: 12px; color: var(--muted); pointer-events: none; }
</style>
