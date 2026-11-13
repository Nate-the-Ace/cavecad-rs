<script lang="ts">
  // Edits one linetype: the lines, spaces, dots and motifs that repeat along a line. One repeat is shown as a strip of blocks
  // to click (and to drag longer or shorter); the part picked is set in the box under it; the preview shows the result on a
  // curve or a straight line at print size.
  import { cave, changed, selectedIds } from '../../lib/cave.svelte';
  import { engine } from '../../lib/engine';
  import { drawing, PATTERN_PREFIX, parseStyle, styleText } from '../../lib/line-style.svelte';
  import { drawMarks, drawMotif, newElement, plain, type Element, type LineDef, type PatternLibrary } from '../../lib/patterns';
  import PatternCanvas from './PatternCanvas.svelte';

  let {
    def,
    lib,
    onsaved,
    onremoved,
    onduplicate,
    ondirty,
    unsaved = false,
  }: {
    def: LineDef;
    lib: PatternLibrary;
    onsaved: (name: string) => void;
    onremoved: () => void;
    onduplicate: () => void;
    ondirty?: (dirty: boolean) => void;
    /** A new pattern or a copy not saved yet: Save is on from the start. */
    unsaved?: boolean;
  } = $props();

  let work = $state<LineDef>(withRows(def));
  /** An editable copy that always has a rows list. */
  function withRows(d: LineDef): LineDef {
    const p = plain(d);
    return { ...p, rows: p.rows ?? [] };
  }
  /** A definition as stored: no rows list when there are no extra rows. */
  function trim(d: LineDef): LineDef {
    const { rows, ...rest } = d;
    return rows?.length ? { ...rest, rows } : rest;
  }
  /** What is being edited, as it would be saved. */
  const out = (): LineDef => trim(plain(work));
  let message = $state('');
  let note = $state('');
  let zoom = $state(2);
  let curved = $state(true);
  /** The part being set in the box under the strip. */
  let sel = $state(0);
  /** The row being edited: 0 is the line itself, 1 on are the rows stacked beside it. */
  let row = $state(0);
  const MAX_ROWS = 6;
  /** The parts of the row being edited. */
  const parts = $derived(row === 0 ? work.elements : (work.rows?.[row - 1]?.elements ?? work.elements));
  function pickRow(r: number) {
    row = r;
    sel = 0;
  }
  function addRow() {
    if ((work.rows?.length ?? 0) >= MAX_ROWS) return;
    // 1 mm further out to the left than the outermost row, so a new row never lands on another
    const outer = Math.max(0, ...work.rows!.map((r) => r.offset_mm));
    work.rows!.push({ offset_mm: Math.round((outer + 1) * 10) / 10, elements: [{ t: 'dash', mm: 6 }] });
    pickRow(work.rows!.length);
  }
  function removeRow() {
    if (row === 0) return;
    work.rows!.splice(row - 1, 1);
    pickRow(Math.min(row, work.rows!.length));
  }
  const offsetText = (mm: number) => (mm === 0 ? 'on the line' : `${Math.abs(mm)} mm ${mm > 0 ? 'left' : 'right'}`);
  const readonly = $derived(!def.own);
  const motifNames = $derived(lib.motifs.map((m) => m.name));
  const problem = $derived.by(() => {
    const r = engine.checkPattern('line', out());
    return r === 'ok' ? '' : r.replace(/^error: /, '');
  });
  const dirty = $derived(!readonly && (unsaved || JSON.stringify(out()) !== JSON.stringify(trim(plain(def)))));
  $effect(() => ondirty?.(dirty));
  const part = $derived(parts[sel]);
  /** One repeat's length on paper. Dots and motifs take no room of their own. */
  const repeatMm = $derived(parts.reduce((s, e) => s + (e.t === 'dash' || e.t === 'gap' ? Math.abs(e.mm) : 0), 0));

  const KIND: Record<Element['t'], string> = { dash: 'Line', gap: 'Space', dot: 'Dot', motif: 'Motif', text: 'Text' };
  const KIND_HELP: Record<Element['t'], string> = {
    dash: 'A drawn stretch of line.',
    gap: 'A stretch left empty.',
    dot: 'A single dot. It takes no room: put a space either side of it.',
    motif: 'A small shape stood on the line (make your own on the Motifs tab). It takes no room: put a space either side of it.',
    text: 'Words written along the line, always turned to read left to right. They take no room: put a space either side, about as long as the words.',
  };

  function add(kind: Element['t']) {
    const at = parts.length ? sel + 1 : 0;
    parts.splice(at, 0, newElement(kind, motifNames[0] ?? ''));
    sel = at;
  }
  function move(by: number) {
    const j = sel + by;
    if (j < 0 || j >= parts.length) return;
    [parts[sel], parts[j]] = [parts[j], parts[sel]];
    sel = j;
  }
  function removePart() {
    parts.splice(sel, 1);
    sel = Math.max(0, Math.min(sel, parts.length - 1));
  }
  function setKind(kind: Element['t']) {
    parts[sel] = newElement(kind, motifNames[0] ?? '');
  }
  function save() {
    const r = engine.savePattern('line', out());
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    if (r === 'ok') {
      changed();
      onsaved(work.name);
    }
  }
  function remove() {
    if (!confirm(`Delete the linetype “${def.name}”? Lines that use it go back to their own look.`)) return;
    const r = engine.removePattern('line', def.name);
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    if (r === 'ok') {
      changed();
      onremoved();
    }
  }

  // Lines selected on the map, to put this linetype on.
  const chosen = $derived.by(() => {
    void cave.also;
    void cave.selected;
    return selectedIds();
  });
  function useOnSelected() {
    let r = 'ok';
    for (const id of chosen) {
      const info = engine.lineInfo(id);
      const out = engine.setLineStyle(id, styleText({ ...parseStyle(info?.style ?? ''), lt: PATTERN_PREFIX + def.name }));
      if (out !== 'ok') r = out;
    }
    changed();
    note = r === 'ok' ? `Put on ${chosen.length} line${chosen.length === 1 ? '' : 's'}. Undo takes it off again.` : r.replace(/^error: /, '');
  }

  /** A gentle wave (or a straight line) 140 mm long on paper. */
  function paint(g: CanvasRenderingContext2D, w: number, h: number) {
    const scale = drawing.scale;
    const pxPerMm = 3.78 * zoom;
    const pxPerM = (pxPerMm * 1000) / scale;
    const mmLong = Math.min(140, (w - 20) / pxPerMm);
    const path: [number, number][] = [];
    for (let i = 0; i <= 120; i++) {
      const x = (mmLong * i) / 120;
      path.push([(x * scale) / 1000, curved ? (Math.sin(x / 14) * 5 * scale) / 1000 : 0]);
    }
    const marks = engine.linePatternMarks(out(), path, scale);
    drawMarks(g, marks, ([x, y]) => [10 + x * pxPerM, h / 2 - y * pxPerM], pxPerM);
  }

  // The strip: each part a block, a line or space as wide as it is long.
  const STRIP_PX_PER_MM = $derived(Math.max(8, Math.min(40, 360 / Math.max(repeatMm, 1))));
  const blockWidth = (e: Element) => (e.t === 'dash' || e.t === 'gap' ? Math.max(14, Math.abs(e.mm) * STRIP_PX_PER_MM) : e.t === 'text' ? Math.max(30, Math.min(90, e.text.length * 9 + 12)) : 30);

  /** Dragging a block's right edge makes that line or space longer or shorter. */
  function grab(e: PointerEvent, i: number) {
    const el = parts[i];
    if (readonly || (el.t !== 'dash' && el.t !== 'gap')) return;
    e.preventDefault();
    e.stopPropagation();
    sel = i;
    const start = e.clientX;
    const was = el.mm;
    const k = STRIP_PX_PER_MM;
    const handle = e.currentTarget as HTMLElement;
    handle.setPointerCapture(e.pointerId);
    const moveTo = (m: PointerEvent) => {
      const now = parts[i];
      if (now.t === 'dash' || now.t === 'gap') now.mm = Math.max(0.1, Math.round((was + (m.clientX - start) / k) * 10) / 10);
    };
    const up = () => {
      handle.removeEventListener('pointermove', moveTo);
      handle.removeEventListener('pointerup', up);
    };
    handle.addEventListener('pointermove', moveTo);
    handle.addEventListener('pointerup', up);
  }

  function motifIcon(name: string) {
    return (g: CanvasRenderingContext2D, w: number, h: number) => {
      const m = lib.motifs.find((x) => x.name === name);
      if (m) drawMotif(g, m.prims, w / 2, h / 2, Math.min(w, h) * 0.7);
    };
  }

  function onKey(e: KeyboardEvent) {
    if (readonly || (e.target as HTMLElement).closest('input, select')) return;
    if (e.key === 'ArrowLeft') sel = Math.max(0, sel - 1);
    else if (e.key === 'ArrowRight') sel = Math.min(parts.length - 1, sel + 1);
    else if (e.key === 'Delete' || e.key === 'Backspace') removePart();
    else return;
    e.preventDefault();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions: arrow keys move between parts, Delete removes one -->
<div class="editor" role="group" aria-label="Linetype editor" onkeydown={onKey}>
  {#if readonly}
    <div class="builtin">
      <span><b>{def.name}</b> comes with CaveCAD and can’t be changed. Make your own copy to change it.</span>
      <button class="primary" onclick={onduplicate}>Make my own copy</button>
    </div>
  {:else}
    <label class="row name">Name <input type="text" bind:value={work.name} maxlength="40" /></label>
  {/if}

  <div class="preview">
    <PatternCanvas draw={paint} height={96} />
    <div class="row small">
      <span class="seg" role="radiogroup" aria-label="Preview line">
        <button role="radio" aria-checked={curved} class:on={curved} onclick={() => (curved = true)}>Curve</button>
        <button role="radio" aria-checked={!curved} class:on={!curved} onclick={() => (curved = false)}>Straight</button>
      </span>
      <span class="seg" role="radiogroup" aria-label="Preview size">
        {#each [[1, 'Actual size'], [2, '2×'], [4, '4×']] as [z, label]}
          <button role="radio" aria-checked={zoom === z} class:on={zoom === z} onclick={() => (zoom = z as number)}>{label}</button>
        {/each}
      </span>
      <span class="muted" title="Printed size depends on the drawing scale in the Sheets tab">at 1:{drawing.scale}</span>
    </div>
  </div>

  <div class="stripbox">
    <div class="rowtabs" role="tablist" aria-label="Rows of the linetype">
      <button role="tab" aria-selected={row === 0} class:on={row === 0} onclick={() => pickRow(0)} title="The line itself">Row 1 <small>on the line</small></button>
      {#each work.rows ?? [] as r, i}
        <button role="tab" aria-selected={row === i + 1} class:on={row === i + 1} onclick={() => pickRow(i + 1)} title="A row running beside the line">Row {i + 2} <small>{offsetText(r.offset_mm)}</small></button>
      {/each}
      {#if !readonly && (work.rows?.length ?? 0) < MAX_ROWS}
        <button class="addrow" onclick={addRow} title="Add another row of parts running beside the line (a double line, ticks beside a wall, a dashed line inside a solid one)">+ Row beside</button>
      {/if}
    </div>
    {#if row > 0 && work.rows?.[row - 1]}
      <label class="field" title="How far this row runs from the drawn line: to the left of the way the line was drawn, or to the right">Offset
        <input type="range" min="-10" max="10" step="0.1" bind:value={work.rows[row - 1].offset_mm} disabled={readonly} />
        <input type="number" step="0.1" bind:value={work.rows[row - 1].offset_mm} disabled={readonly} /> mm
      </label>
      <div class="row small">
        <span class="muted">More than 0 is to the left of the way the line was drawn, less than 0 to the right.</span>
        {#if !readonly}<button class="danger small-btn" onclick={removeRow}>Remove this row</button>{/if}
      </div>
    {/if}
    <div class="row small">
      <b>One repeat{(work.rows?.length ?? 0) ? ` of row ${row + 1}` : ''}</b>
      <span class="muted">{repeatMm.toFixed(1)} mm on paper, then it starts again{readonly ? '' : '. Click a part to change it; drag its right edge to make a line or space longer.'}</span>
    </div>
    <div class="strip" role="listbox" aria-label="Parts of one repeat" tabindex="-1">
      {#each parts as el, i}
        <div
          class="block {el.t}"
          class:sel={i === sel}
          style="width:{blockWidth(el)}px"
          role="option"
          aria-selected={i === sel}
          tabindex="0"
          title="{KIND[el.t]}{el.t === 'dash' || el.t === 'gap' ? ` · ${el.mm} mm` : el.t === 'motif' ? ` · ${el.motif}` : el.t === 'text' ? ` · ${el.text}` : ''}"
          onclick={() => (sel = i)}
          onkeydown={(e) => e.key === 'Enter' && (sel = i)}
        >
          {#if el.t === 'dash'}<span class="ink"></span>
          {:else if el.t === 'dot'}<span class="dot"></span>
          {:else if el.t === 'motif'}<span class="icon"><PatternCanvas draw={motifIcon(el.motif)} height={22} /></span>
          {:else if el.t === 'text'}<span class="words">{el.text || '…'}</span>
          {/if}
          {#if !readonly && (el.t === 'dash' || el.t === 'gap')}
            <span class="grip" role="separator" aria-label="Drag to change the length" onpointerdown={(e) => grab(e, i)}></span>
          {/if}
        </div>
      {/each}
      {#if !parts.length}<span class="muted">No parts yet: add one below.</span>{/if}
    </div>
    {#if !readonly}
      <div class="row add">
        <span class="muted">Add after the picked part:</span>
        <button onclick={() => add('dash')} title={KIND_HELP.dash}>+ Line</button>
        <button onclick={() => add('gap')} title={KIND_HELP.gap}>+ Space</button>
        <button onclick={() => add('dot')} title={KIND_HELP.dot}>+ Dot</button>
        <button onclick={() => add('motif')} disabled={!motifNames.length} title={KIND_HELP.motif}>+ Motif</button>
        <button onclick={() => add('text')} title={KIND_HELP.text}>+ Text</button>
      </div>
    {/if}
  </div>

  {#if part}
    <div class="partbox">
      <div class="row">
        <b>Part {sel + 1} of {parts.length}:</b>
        <select value={part.t} onchange={(e) => setKind(e.currentTarget.value as Element['t'])} disabled={readonly} aria-label="Kind of part">
          {#each Object.entries(KIND) as [k, label]}<option value={k}>{label}</option>{/each}
        </select>
        {#if !readonly}
          <span class="btns">
            <button onclick={() => move(-1)} disabled={sel === 0} title="Move this part earlier in the repeat (←)">◀ Earlier</button>
            <button onclick={() => move(1)} disabled={sel >= parts.length - 1} title="Move this part later in the repeat (→)">Later ▶</button>
            <button onclick={removePart} title="Take this part out (Delete)">Remove</button>
          </span>
        {/if}
      </div>
      <p class="muted">{KIND_HELP[part.t]}</p>
      {#if part.t === 'dash' || part.t === 'gap'}
        <label class="field">Length on paper
          <input type="range" min="0.1" max="20" step="0.1" bind:value={part.mm} disabled={readonly} />
          <input type="number" step="0.1" min="0.1" bind:value={part.mm} disabled={readonly} /> mm
        </label>
      {:else if part.t === 'text'}
        <label class="field">Words
          <input class="wide" type="text" bind:value={part.text} maxlength="24" disabled={readonly} placeholder="for example ?  W  DIG" />
        </label>
        <label class="field">Letter height
          <input type="range" min="0.5" max="10" step="0.1" bind:value={part.size_mm} disabled={readonly} />
          <input type="number" step="0.1" min="0.5" bind:value={part.size_mm} disabled={readonly} /> mm
        </label>
        <label class="field">Rotate
          <input type="range" min="-180" max="180" step="5" bind:value={part.turn_deg} disabled={readonly} />
          <input type="number" step="5" bind:value={part.turn_deg} disabled={readonly} /> °
        </label>
        <label class="field" title="Moves the words forward (more) or back (less) along the line, without taking room">Nudge along
          <input type="range" min="-10" max="10" step="0.1" bind:value={part.along_mm} disabled={readonly} />
          <input type="number" step="0.1" bind:value={part.along_mm} disabled={readonly} /> mm
        </label>
        <label class="field" title="Moves the words off the line: more is to the left of the way the line was drawn">Nudge sideways
          <input type="range" min="-10" max="10" step="0.1" bind:value={part.across_mm} disabled={readonly} />
          <input type="number" step="0.1" bind:value={part.across_mm} disabled={readonly} /> mm
        </label>
      {:else if part.t === 'motif'}
        <label class="field">Motif
          <select bind:value={part.motif} disabled={readonly} aria-label="Motif">
            {#each motifNames as n}<option value={n}>{n}</option>{/each}
            {#if !motifNames.includes(part.motif)}<option value={part.motif}>{part.motif} (missing)</option>{/if}
          </select>
        </label>
        <label class="field">Size
          <input type="range" min="0.2" max="10" step="0.1" bind:value={part.size_mm} disabled={readonly} />
          <input type="number" step="0.1" min="0.2" bind:value={part.size_mm} disabled={readonly} /> mm
        </label>
        <label class="field">Rotate
          <input type="range" min="-180" max="180" step="5" bind:value={part.turn_deg} disabled={readonly} />
          <input type="number" step="5" bind:value={part.turn_deg} disabled={readonly} /> °
        </label>
        <label class="field" title="Moves the motif forward (more) or back (less) along the line, without taking room">Nudge along
          <input type="range" min="-10" max="10" step="0.1" bind:value={part.along_mm} disabled={readonly} />
          <input type="number" step="0.1" bind:value={part.along_mm} disabled={readonly} /> mm
        </label>
        <label class="field" title="Moves the motif off the line: more is to the left of the way the line was drawn">Nudge sideways
          <input type="range" min="-10" max="10" step="0.1" bind:value={part.across_mm} disabled={readonly} />
          <input type="number" step="0.1" bind:value={part.across_mm} disabled={readonly} /> mm
        </label>
      {/if}
    </div>
  {/if}

  <div class="footer">
    {#if problem}<p class="problem">{problem}</p>{/if}
    {#if message}<p class="problem">{message}</p>{/if}
    {#if !readonly}
      <div class="row actions">
        <button class="primary" onclick={save} disabled={!!problem || !dirty}>Save</button>
        <button onclick={() => ((work = withRows(def)), (sel = 0), (row = 0))} disabled={!dirty || unsaved} title="Go back to how it was last saved">Undo changes</button>
        {#if dirty}<span class="unsaved">Not saved yet</span>{/if}
        {#if !unsaved}<button class="danger" onclick={remove}>Delete…</button>{/if}
      </div>
    {/if}
    {#if !readonly && !dirty}
      <div class="row use">
        <button onclick={useOnSelected} disabled={!chosen.length} title={chosen.length ? 'Give the lines selected on the map this linetype' : 'Select lines on the map first'}>
          Use on {chosen.length ? `${chosen.length} selected line${chosen.length === 1 ? '' : 's'}` : 'selected lines'}
        </button>
        {#if !chosen.length}<span class="muted">Select lines on the map first.</span>{/if}
      </div>
    {/if}
    {#if note}<p class="note">{note}</p>{/if}
  </div>
</div>

<style>
  .editor { display: flex; flex-direction: column; gap: 10px; outline: none; }
  .row { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
  .small { font-size: 11px; }
  .muted { color: var(--muted); font-size: 11px; }
  .name input { flex: 1; min-width: 120px; }
  .builtin { display: flex; gap: 8px; align-items: center; flex-wrap: wrap; padding: 8px; border: 1px solid var(--accent); border-radius: var(--radius); background: var(--accent-bg); font-size: 12px; }
  .builtin span { flex: 1; min-width: 180px; }
  .preview { display: flex; flex-direction: column; gap: 4px; }
  .seg { display: inline-flex; border: 1px solid var(--line); border-radius: var(--radius); overflow: hidden; }
  .seg button { border: none; border-radius: 0; padding: 2px 8px; font-size: 11px; background: var(--panel-2); color: var(--muted); }
  .seg button.on { background: var(--accent-bg); color: var(--text); }
  .stripbox, .partbox { display: flex; flex-direction: column; gap: 6px; padding: 8px; border: 1px solid var(--line); border-radius: var(--radius); }
  .strip { display: flex; align-items: stretch; gap: 3px; overflow-x: auto; padding: 4px 2px; min-height: 40px; }
  .block { position: relative; flex: none; height: 34px; display: flex; align-items: center; justify-content: center; border: 1px solid var(--line); border-radius: 4px; background: var(--panel-2); cursor: pointer; }
  .block.gap { background: repeating-linear-gradient(45deg, transparent 0 4px, color-mix(in srgb, var(--line) 55%, transparent) 4px 5px); }
  .block.sel { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-bg); }
  .block:focus-visible { outline: 2px solid var(--accent); }
  .ink { display: block; height: 3px; width: calc(100% - 8px); background: var(--text); border-radius: 2px; }
  .dot { width: 7px; height: 7px; border-radius: 50%; background: var(--text); }
  .icon { width: 26px; }
  .rowtabs { display: flex; gap: 3px; flex-wrap: wrap; }
  .rowtabs button { font-size: 11px; padding: 2px 8px; color: var(--muted); }
  .rowtabs button.on { background: var(--accent-bg); border-color: var(--accent); color: var(--text); }
  .rowtabs small { display: block; font-size: 9px; color: var(--muted); }
  .addrow { border-style: dashed; }
  .small-btn { font-size: 11px; padding: 1px 6px; margin-left: auto; }
  .danger { color: var(--danger, #c0392b); }
  .words { font-size: 12px; font-weight: 600; max-width: 100%; overflow: hidden; white-space: nowrap; padding: 0 3px; }
  .field .wide { grid-column: 2 / 5; }
  .icon :global(canvas) { border: none; background: none; }
  .grip { position: absolute; right: -4px; top: 4px; bottom: 4px; width: 8px; cursor: ew-resize; border-radius: 3px; background: var(--accent); opacity: 0; }
  .block:hover .grip, .block.sel .grip { opacity: 0.85; }
  .add button { font-size: 12px; }
  .partbox p { margin: 0; }
  .field { display: grid; grid-template-columns: 110px 1fr 64px auto; gap: 6px; align-items: center; font-size: 12px; color: var(--muted); }
  .field select { grid-column: 2 / 4; }
  input[type='number'] { width: 64px; }
  input[type='range'] { width: 100%; padding: 0; }
  input, select, button { font: inherit; padding: 3px 6px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); }
  button { cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  .primary { background: var(--accent-bg); border-color: var(--accent); }
  .btns { margin-left: auto; display: flex; gap: 3px; }
  .btns button { font-size: 11px; }
  .actions .danger { margin-left: auto; color: var(--danger, #c0392b); }
  .unsaved { font-size: 11px; color: var(--warn, #d08a00); }
  .problem { color: var(--danger, #c0392b); font-size: 12px; margin: 0; }
  .note { font-size: 12px; color: var(--muted); margin: 0; }
  .footer { position: sticky; bottom: 0; display: flex; flex-direction: column; gap: 6px; padding: 8px 0 2px; background: var(--panel); border-top: 1px solid var(--line); }
</style>
