<script lang="ts">
  // Edits one linetype: the dashes, gaps, dots and motifs that repeat along a line, with a live preview at print size.
  import { changed } from '../../lib/cave.svelte';
  import { engine } from '../../lib/engine';
  import { drawing } from '../../lib/line-style.svelte';
  import { drawMarks, newElement, plain, type Element, type LineDef, type PatternLibrary } from '../../lib/patterns';
  import PatternCanvas from './PatternCanvas.svelte';

  let { def, lib, onsaved, onremoved, onduplicate }: { def: LineDef; lib: PatternLibrary; onsaved: (name: string) => void; onremoved: () => void; onduplicate: () => void } = $props();

  let work = $state<LineDef>(plain(def));
  let message = $state('');
  let zoom = $state(2);
  const readonly = $derived(!def.own);
  const motifNames = $derived(lib.motifs.map((m) => m.name));
  const problem = $derived.by(() => {
    const r = engine.checkPattern('line', plain(work));
    return r === 'ok' ? '' : r.replace(/^error: /, '');
  });
  const dirty = $derived(JSON.stringify(plain(work)) !== JSON.stringify(plain(def)));

  function add(kind: Element['t']) {
    work.elements.push(newElement(kind, motifNames[0] ?? ''));
  }
  function move(i: number, by: number) {
    const j = i + by;
    if (j < 0 || j >= work.elements.length) return;
    [work.elements[i], work.elements[j]] = [work.elements[j], work.elements[i]];
  }
  function setKind(i: number, kind: Element['t']) {
    work.elements[i] = newElement(kind, motifNames[0] ?? '');
  }
  function save() {
    const r = engine.savePattern('line', plain(work));
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    if (r === 'ok') {
      changed();
      onsaved(work.name);
    }
  }
  function remove() {
    const r = engine.removePattern('line', def.name);
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    if (r === 'ok') {
      changed();
      onremoved();
    }
  }

  /** A gentle wave 140 mm long on paper, so the pattern is judged on a curve as well as straight. */
  function paint(g: CanvasRenderingContext2D, w: number, h: number) {
    const scale = drawing.scale;
    const pxPerMm = 3.78 * zoom;
    const pxPerM = (pxPerMm * 1000) / scale;
    const mmLong = Math.min(140, (w - 20) / pxPerMm);
    const path: [number, number][] = [];
    for (let i = 0; i <= 120; i++) {
      const x = (mmLong * i) / 120;
      path.push([(x * scale) / 1000, (Math.sin(x / 14) * 5 * scale) / 1000]);
    }
    const marks = engine.linePatternMarks(plain(work), path, scale);
    drawMarks(g, marks, ([x, y]) => [10 + x * pxPerM, h / 2 - y * pxPerM], pxPerM);
  }
</script>

<div class="editor">
  <label class="row">Name <input type="text" bind:value={work.name} maxlength="40" disabled={readonly} /></label>
  <PatternCanvas draw={paint} height={96} />
  <div class="row small">
    <span>Shown at 1:{drawing.scale}, printed size ×</span>
    <select bind:value={zoom} aria-label="Preview zoom"><option value={1}>1</option><option value={2}>2</option><option value={4}>4</option></select>
  </div>
  {#if readonly}
    <p class="note">This one comes with CaveCAD. <button onclick={onduplicate}>Duplicate to change it</button></p>
  {/if}
  <ol class="parts">
    {#each work.elements as el, i}
      <li>
        <select value={el.t} onchange={(e) => setKind(i, e.currentTarget.value as Element['t'])} disabled={readonly} aria-label="Kind of part">
          <option value="dash">Dash</option><option value="gap">Gap</option><option value="dot">Dot</option><option value="motif">Motif</option>
        </select>
        {#if el.t === 'dash' || el.t === 'gap'}
          <label>length mm <input type="number" step="0.1" min="0.05" bind:value={el.mm} disabled={readonly} /></label>
        {:else if el.t === 'motif'}
          <select bind:value={el.motif} disabled={readonly} aria-label="Motif">
            {#each motifNames as n}<option value={n}>{n}</option>{/each}
            {#if !motifNames.includes(el.motif)}<option value={el.motif}>{el.motif} (missing)</option>{/if}
          </select>
          <label>size <input type="number" step="0.1" min="0.2" bind:value={el.size_mm} disabled={readonly} /></label>
          <label>turn° <input type="number" step="5" bind:value={el.turn_deg} disabled={readonly} /></label>
          <label>along <input type="number" step="0.1" bind:value={el.along_mm} disabled={readonly} /></label>
          <label>across <input type="number" step="0.1" bind:value={el.across_mm} disabled={readonly} /></label>
        {:else}
          <span class="hint">a dot</span>
        {/if}
        {#if !readonly}
          <span class="btns">
            <button onclick={() => move(i, -1)} aria-label="Move up" title="Move up">↑</button>
            <button onclick={() => move(i, 1)} aria-label="Move down" title="Move down">↓</button>
            <button onclick={() => work.elements.splice(i, 1)} aria-label="Remove part" title="Remove">✕</button>
          </span>
        {/if}
      </li>
    {/each}
  </ol>
  {#if !readonly}
    <div class="row">
      <button onclick={() => add('dash')}>+ Dash</button><button onclick={() => add('gap')}>+ Gap</button><button onclick={() => add('dot')}>+ Dot</button><button onclick={() => add('motif')} disabled={!motifNames.length}>+ Motif</button>
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
  button { cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  .primary { background: var(--accent-bg); border-color: var(--accent); }
  .btns { margin-left: auto; display: flex; gap: 2px; }
  .problem { color: var(--danger, #c0392b); font-size: 12px; margin: 0; }
  .note, .hint { font-size: 12px; color: var(--muted); margin: 0; }
</style>
