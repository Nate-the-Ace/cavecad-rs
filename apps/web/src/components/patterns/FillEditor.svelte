<script lang="ts">
  // Edits one fill: layers of hatch lines, dots and tiled motifs laid inside an outline, with a live preview at print size.
  import { changed } from '../../lib/cave.svelte';
  import { engine } from '../../lib/engine';
  import { drawing } from '../../lib/line-style.svelte';
  import { drawMarks, newLayer, plain, type FillDef, type Layer, type PatternLibrary } from '../../lib/patterns';
  import PatternCanvas from './PatternCanvas.svelte';

  let { def, lib, onsaved, onremoved, onduplicate }: { def: FillDef; lib: PatternLibrary; onsaved: (name: string) => void; onremoved: () => void; onduplicate: () => void } = $props();

  let work = $state<FillDef>(plain(def));
  let message = $state('');
  let zoom = $state(2);
  let amount = $state(1);
  const readonly = $derived(!def.own);
  const motifNames = $derived(lib.motifs.map((m) => m.name));
  const problem = $derived.by(() => {
    const r = engine.checkPattern('fill', plain(work));
    return r === 'ok' ? '' : r.replace(/^error: /, '');
  });
  const dirty = $derived(JSON.stringify(plain(work)) !== JSON.stringify(plain(def)));

  function add(kind: Layer['t']) {
    work.layers.push(newLayer(kind, motifNames[0] ?? ''));
  }
  function setKind(i: number, kind: Layer['t']) {
    work.layers[i] = newLayer(kind, motifNames[0] ?? '');
  }
  function move(i: number, by: number) {
    const j = i + by;
    if (j < 0 || j >= work.layers.length) return;
    [work.layers[i], work.layers[j]] = [work.layers[j], work.layers[i]];
  }
  function save() {
    const r = engine.savePattern('fill', plain(work));
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    if (r === 'ok') {
      changed();
      onsaved(work.name);
    }
  }
  function remove() {
    const r = engine.removePattern('fill', def.name);
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    if (r === 'ok') {
      changed();
      onremoved();
    }
  }

  /** A rounded blob 90 by 55 mm on paper, so the fill is judged against a curved edge. */
  function paint(g: CanvasRenderingContext2D, w: number, h: number) {
    const scale = drawing.scale;
    const pxPerMm = 3.78 * zoom;
    const pxPerM = (pxPerMm * 1000) / scale;
    const outline: [number, number][] = [];
    for (let i = 0; i < 48; i++) {
      const a = (i / 48) * Math.PI * 2;
      const r = 1 + 0.12 * Math.sin(3 * a) + 0.07 * Math.cos(5 * a);
      outline.push([(Math.cos(a) * r * 45 * scale) / 1000, (Math.sin(a) * r * 27 * scale) / 1000]);
    }
    const marks = engine.fillPatternMarks(plain(work), outline, 'preview', amount, scale);
    const px = ([x, y]: [number, number]): [number, number] => [w / 2 + x * pxPerM, h / 2 - y * pxPerM];
    g.save();
    g.beginPath();
    outline.forEach((p, i) => {
      const [x, y] = px(p);
      if (i) g.lineTo(x, y);
      else g.moveTo(x, y);
    });
    g.closePath();
    if (marks.tint) {
      g.globalAlpha = 0.25;
      g.fill();
      g.globalAlpha = 1;
    }
    g.stroke();
    g.clip();
    drawMarks(g, marks, px, pxPerM);
    g.restore();
  }
</script>

<div class="editor">
  <label class="row">Name <input type="text" bind:value={work.name} maxlength="40" disabled={readonly} /></label>
  <PatternCanvas draw={paint} height={Math.round(55 * 3.78 * zoom * 1.2)} />
  <div class="row small">
    <span>Shown at 1:{drawing.scale}, printed size ×</span>
    <select bind:value={zoom} aria-label="Preview zoom"><option value={1}>1</option><option value={2}>2</option></select>
    <label>denser <input type="range" min="0.5" max="3" step="0.1" bind:value={amount} /></label>
  </div>
  {#if readonly}
    <p class="note">This one comes with CaveCAD. <button onclick={onduplicate}>Duplicate to change it</button></p>
  {/if}
  <ol class="parts">
    {#each work.layers as l, i}
      <li>
        <select value={l.t} onchange={(e) => setKind(i, e.currentTarget.value as Layer['t'])} disabled={readonly} aria-label="Kind of layer">
          <option value="hatch">Hatch lines</option><option value="dots">Dots</option><option value="tile">Motif tiles</option>
        </select>
        {#if l.t === 'hatch'}
          <label>angle° <input type="number" step="5" bind:value={l.angle_deg} disabled={readonly} /></label>
          <label>spacing mm <input type="number" step="0.1" min="0.2" bind:value={l.spacing_mm} disabled={readonly} /></label>
        {:else if l.t === 'dots'}
          <label>spacing mm <input type="number" step="0.1" min="0.2" bind:value={l.spacing_mm} disabled={readonly} /></label>
          <label>dot mm <input type="number" step="0.05" min="0.05" bind:value={l.dot_mm} disabled={readonly} /></label>
        {:else}
          <select bind:value={l.motif} disabled={readonly} aria-label="Motif">
            {#each motifNames as n}<option value={n}>{n}</option>{/each}
            {#if !motifNames.includes(l.motif)}<option value={l.motif}>{l.motif} (missing)</option>{/if}
          </select>
          <label>size <input type="number" step="0.1" min="0.2" bind:value={l.size_mm} disabled={readonly} /></label>
          <label>across <input type="number" step="0.5" min="0.5" bind:value={l.across_mm} disabled={readonly} /></label>
          <label>down <input type="number" step="0.5" min="0.5" bind:value={l.down_mm} disabled={readonly} /></label>
          <label><input type="checkbox" bind:checked={l.stagger} disabled={readonly} /> stagger</label>
          <label><input type="checkbox" bind:checked={l.random_turn} disabled={readonly} /> random turn</label>
          {#if !l.random_turn}<label>turn° <input type="number" step="5" bind:value={l.turn_deg} disabled={readonly} /></label>{/if}
          <label>size var <input type="number" step="0.1" min="0" max="1" bind:value={l.size_var} disabled={readonly} /></label>
          <label>wander <input type="number" step="0.1" min="0" max="1" bind:value={l.jitter} disabled={readonly} /></label>
          <label>leave out <input type="number" step="0.05" min="0" max="0.9" bind:value={l.skip} disabled={readonly} /></label>
        {/if}
        {#if !readonly}
          <span class="btns">
            <button onclick={() => move(i, -1)} aria-label="Move up" title="Move up">↑</button>
            <button onclick={() => move(i, 1)} aria-label="Move down" title="Move down">↓</button>
            <button onclick={() => work.layers.splice(i, 1)} aria-label="Remove layer" title="Remove">✕</button>
          </span>
        {/if}
      </li>
    {/each}
  </ol>
  {#if !readonly}
    <div class="row">
      <button onclick={() => add('hatch')}>+ Hatch</button><button onclick={() => add('dots')}>+ Dots</button><button onclick={() => add('tile')} disabled={!motifNames.length}>+ Motif tiles</button>
      <label class="small"><input type="checkbox" bind:checked={work.tint} /> tint the whole area</label>
    </div>
  {/if}
  {#if problem}<p class="problem">{problem}</p>{/if}
  {#if message}<p class="problem">{message}</p>{/if}
  {#if !readonly}
    <div class="row">
      <button class="primary" onclick={save} disabled={!!problem || !dirty}>Save</button>
      <button onclick={() => (work = plain(def))} disabled={!dirty}>Revert</button>
      <button onclick={remove}>Delete</button>
    </div>
  {/if}
</div>

<style>
  .editor { display: flex; flex-direction: column; gap: 8px; }
  .row { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  .small { font-size: 11px; color: var(--muted); }
  .parts { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 4px; }
  .parts li { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; padding: 4px 6px; border: 1px solid var(--line); border-radius: var(--radius); }
  .parts label { font-size: 11px; color: var(--muted); display: flex; gap: 3px; align-items: center; }
  input[type='number'] { width: 58px; }
  input, select, button { font: inherit; padding: 3px 6px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); }
  input[type='text'] { flex: 1; min-width: 120px; }
  input[type='checkbox'], input[type='range'] { padding: 0; }
  button { cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  .primary { background: var(--accent-bg); border-color: var(--accent); }
  .btns { margin-left: auto; display: flex; gap: 2px; }
  .problem { color: var(--danger, #c0392b); font-size: 12px; margin: 0; }
  .note { font-size: 12px; color: var(--muted); margin: 0; }
</style>
