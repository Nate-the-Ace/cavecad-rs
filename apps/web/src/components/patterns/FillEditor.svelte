<script lang="ts">
  // Edits one fill: layers of hatch lines, dots and scattered motifs laid inside an outline. Each layer is a card with its own
  // small sample; the layer picked is set in the box under them; the preview shows all layers together inside a rounded area at
  // print size.
  import { cave, changed, selectedIds, setTool } from '../../lib/cave.svelte';
  import { engine } from '../../lib/engine';
  import { isFillKind } from '../../lib/fills';
  import { drawing, PATTERN_PREFIX, parseStyle, styleText } from '../../lib/line-style.svelte';
  import { drawMarks, newLayer, plain, type FillDef, type Layer, type PatternLibrary } from '../../lib/patterns';
  import PatternCanvas from './PatternCanvas.svelte';

  let {
    def,
    lib,
    onsaved,
    onremoved,
    onduplicate,
    ondirty,
    unsaved = false,
    fromLibrary = false,
  }: {
    def: FillDef;
    lib: PatternLibrary;
    onsaved: (name: string) => void;
    onremoved: () => void;
    onduplicate: () => void;
    ondirty?: (dirty: boolean) => void;
    /** A new fill or a copy not saved yet: Save is on from the start. */
    unsaved?: boolean;
    /** Shown from the account library, not in this cave yet: the banner offers Use in this cave. */
    fromLibrary?: boolean;
  } = $props();

  let work = $state<FillDef>(plain(def));
  let message = $state('');
  let note = $state('');
  let zoom = $state(1);
  let amount = $state(1);
  /** The layer being set in the box under the cards. */
  let sel = $state(0);
  const readonly = $derived(!def.own);
  const motifNames = $derived(lib.motifs.map((m) => m.name));
  const problem = $derived.by(() => {
    const r = engine.checkPattern('fill', plain(work));
    return r === 'ok' ? '' : r.replace(/^error: /, '');
  });
  const dirty = $derived(!readonly && (unsaved || JSON.stringify(plain(work)) !== JSON.stringify(plain(def))));
  $effect(() => ondirty?.(dirty));
  const layer = $derived(work.layers[sel]);

  const KIND: Record<Layer['t'], string> = { hatch: 'Hatch lines', dots: 'Dots', tile: 'Scattered motifs', patlines: 'Pattern lines' };
  const KIND_HELP: Record<Layer['t'], string> = {
    hatch: 'Parallel lines across the whole area.',
    dots: 'Dots in an even grid.',
    tile: 'A motif repeated across the area in rows, with as much randomness as you like.',
    patlines: 'Rows of dashed lines, the way an AutoCAD .pat pattern line works: each row can start further along than the one before.',
  };
  /** The dash list as typed: numbers with commas, more than 0 a dash, less than 0 a gap, 0 a dot. */
  const dashText = (d: number[]) => d.join(', ');
  function setDashes(text: string) {
    const l = work.layers[sel];
    if (l?.t !== 'patlines') return;
    const nums = text.split(/[,\s]+/).filter(Boolean).map(Number);
    if (nums.every((n) => Number.isFinite(n))) l.dashes_mm = nums;
  }

  function add(kind: Layer['t']) {
    const at = work.layers.length ? sel + 1 : 0;
    work.layers.splice(at, 0, newLayer(kind, motifNames[0] ?? ''));
    sel = at;
  }
  /** Every fill but this one, to stack into it. */
  const others = $derived(lib.fills.filter((f) => f.name !== def.name));
  /** Adds all of another fill's layers on top of this one's (a mix of patterns in one fill). */
  function stack(name: string) {
    const from = lib.fills.find((f) => f.name === name);
    if (!from) return;
    const room = 32 - work.layers.length;
    const add = (JSON.parse(JSON.stringify(from.layers)) as Layer[]).slice(0, Math.max(0, room));
    if (!add.length) {
      message = 'This fill already has as many layers as it can hold (32).';
      return;
    }
    work.layers.push(...add);
    if (from.tint) work.tint = true;
    sel = work.layers.length - add.length;
    note = `Added ${add.length} layer${add.length === 1 ? '' : 's'} from “${name}”${add.length < from.layers.length ? ` (only ${add.length} fitted)` : ''}. Each can now be changed on its own.`;
  }

  function move(by: number) {
    const j = sel + by;
    if (j < 0 || j >= work.layers.length) return;
    [work.layers[sel], work.layers[j]] = [work.layers[j], work.layers[sel]];
    sel = j;
  }
  function removeLayer() {
    work.layers.splice(sel, 1);
    sel = Math.max(0, Math.min(sel, work.layers.length - 1));
  }
  function setKind(kind: Layer['t']) {
    work.layers[sel] = newLayer(kind, motifNames[0] ?? '');
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
    if (!confirm(`Delete the fill “${def.name}”? Areas that use it lose their pattern.`)) return;
    const r = engine.removePattern('fill', def.name);
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    if (r === 'ok') {
      changed();
      onremoved();
    }
  }

  // Areas selected on the map, to fill with this.
  const chosenAreas = $derived.by(() => {
    void cave.also;
    void cave.selected;
    void cave.version;
    return selectedIds().filter((id) => isFillKind(engine.lineInfo(id)?.kind));
  });
  function useOnSelected() {
    let r = 'ok';
    for (const id of chosenAreas) {
      const info = engine.lineInfo(id);
      if (info?.kind !== 'fill-custom') {
        const k = engine.setLineKind(id, 'fill-custom');
        if (k !== 'ok') r = k;
      }
      const out = engine.setLineStyle(id, styleText({ ...parseStyle(engine.lineInfo(id)?.style ?? ''), lt: PATTERN_PREFIX + def.name }));
      if (out !== 'ok') r = out;
    }
    changed();
    const n = chosenAreas.length;
    note = r === 'ok' ? `Filled ${n} area${n === 1 ? '' : 's'}. Undo takes it off again.` : r.replace(/^error: /, '');
  }
  function drawWith() {
    cave.options.customFill = def.name;
    if (cave.tool !== 'fill-custom') setTool('fill-custom');
    note = 'Now draw all the way around an area in the Map, Profile or Cross Section.';
  }

  /** A rounded blob 90 by 55 mm on paper. */
  const blob = (scale: number): [number, number][] =>
    Array.from({ length: 48 }, (_, i) => {
      const a = (i / 48) * Math.PI * 2;
      const r = 1 + 0.12 * Math.sin(3 * a) + 0.07 * Math.cos(5 * a);
      return [(Math.cos(a) * r * 45 * scale) / 1000, (Math.sin(a) * r * 27 * scale) / 1000] as [number, number];
    });

  /** Fills `outline` (metres) with `d` and draws it, centred, `pxPerMm` pixels to a paper millimetre. */
  function drawFill(g: CanvasRenderingContext2D, w: number, h: number, d: FillDef, outline: [number, number][], pxPerMm: number, edge: boolean) {
    const scale = drawing.scale;
    const pxPerM = (pxPerMm * 1000) / scale;
    let marks;
    try {
      marks = engine.fillPatternMarks(plain(d), outline, 'preview', amount, scale);
    } catch {
      return;
    }
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
    if (edge) g.stroke();
    g.clip();
    drawMarks(g, marks, px, pxPerM);
    g.restore();
  }

  function paint(g: CanvasRenderingContext2D, w: number, h: number) {
    drawFill(g, w, h, work, blob(drawing.scale), 3.78 * zoom, true);
  }

  /** One layer alone, filling a small card. */
  function layerSample(i: number) {
    return (g: CanvasRenderingContext2D, w: number, h: number) => {
      const l = work.layers[i];
      if (!l) return;
      const pxPerMm = 3.78;
      const hx = (w / 2 / pxPerMm) * (drawing.scale / 1000);
      const hy = (h / 2 / pxPerMm) * (drawing.scale / 1000);
      const box: [number, number][] = [
        [-hx, -hy],
        [hx, -hy],
        [hx, hy],
        [-hx, hy],
      ];
      drawFill(g, w, h, { name: 'sample', layers: [JSON.parse(JSON.stringify(l)) as Layer], tint: false }, box, pxPerMm, false);
    };
  }

  function onKey(e: KeyboardEvent) {
    if (readonly || (e.target as HTMLElement).closest('input, select')) return;
    if (e.key === 'ArrowLeft') sel = Math.max(0, sel - 1);
    else if (e.key === 'ArrowRight') sel = Math.min(work.layers.length - 1, sel + 1);
    else if (e.key === 'Delete' || e.key === 'Backspace') removeLayer();
    else return;
    e.preventDefault();
  }
  const pct = (v: number) => `${Math.round(v * 100)}%`;
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions: arrow keys move between layers, Delete removes one -->
<div class="editor" role="group" aria-label="Fill editor" onkeydown={onKey}>
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
    <label class="row name">Name <input type="text" bind:value={work.name} maxlength="40" /></label>
  {/if}

  <div class="preview">
    <PatternCanvas draw={paint} height={Math.round(55 * 3.78 * zoom * 1.15)} />
    <div class="row small">
      <span class="seg" role="radiogroup" aria-label="Preview size">
        {#each [[1, 'Actual size'], [2, '2×']] as [z, label]}
          <button role="radio" aria-checked={zoom === z} class:on={zoom === z} onclick={() => (zoom = z as number)}>{label}</button>
        {/each}
      </span>
      <label class="try" title="Each area can be made denser or sparser on the map; try it here. This changes only the preview.">Try it denser
        <input type="range" min="0.5" max="3" step="0.1" bind:value={amount} />
      </label>
      <span class="muted">at 1:{drawing.scale}</span>
    </div>
  </div>

  <div class="stripbox">
    <div class="row small">
      <b>Layers</b>
      <span class="muted">drawn on top of each other{readonly ? '' : '. Click one to change it.'}</span>
    </div>
    <div class="cards" role="listbox" aria-label="Layers of the fill" tabindex="-1">
      {#each work.layers as l, i}
        <div class="card" class:sel={i === sel} role="option" aria-selected={i === sel} tabindex="0" onclick={() => (sel = i)} onkeydown={(e) => e.key === 'Enter' && (sel = i)} title={KIND[l.t]}>
          <PatternCanvas draw={layerSample(i)} height={52} />
          <span>{KIND[l.t]}</span>
        </div>
      {/each}
      {#if !work.layers.length}<span class="muted">No layers yet: add one below.</span>{/if}
    </div>
    {#if !readonly}
      <div class="row add">
        <span class="muted">Add a layer:</span>
        <button onclick={() => add('hatch')} title={KIND_HELP.hatch}>+ Hatch lines</button>
        <button onclick={() => add('dots')} title={KIND_HELP.dots}>+ Dots</button>
        <button onclick={() => add('tile')} disabled={!motifNames.length} title={KIND_HELP.tile}>+ Scattered motifs</button>
        <button onclick={() => add('patlines')} title={KIND_HELP.patlines}>+ Pattern lines</button>
      </div>
      <div class="row add">
        <span class="muted">Mix in another pattern:</span>
        <select
          aria-label="Add a whole pattern"
          title="Adds all the layers of another fill on top of this one, so two or more patterns show in one area"
          value=""
          onchange={(e) => {
            const v = e.currentTarget.value;
            e.currentTarget.value = '';
            if (v) stack(v);
          }}
        >
          <option value="">Add a whole pattern…</option>
          {#each others as f}<option value={f.name}>{f.name}</option>{/each}
        </select>
      </div>
      <label class="check"><input type="checkbox" bind:checked={work.tint} /> Shade the whole area lightly as well</label>
    {/if}
  </div>

  {#if layer}
    <div class="partbox">
      <div class="row">
        <b>Layer {sel + 1} of {work.layers.length}:</b>
        <select value={layer.t} onchange={(e) => setKind(e.currentTarget.value as Layer['t'])} disabled={readonly} aria-label="Kind of layer">
          {#each Object.entries(KIND) as [k, label]}<option value={k}>{label}</option>{/each}
        </select>
        {#if !readonly}
          <span class="btns">
            <button onclick={() => move(-1)} disabled={sel === 0} title="Draw this layer earlier (under the others)">◀ Earlier</button>
            <button onclick={() => move(1)} disabled={sel >= work.layers.length - 1} title="Draw this layer later (over the others)">Later ▶</button>
            <button onclick={removeLayer} title="Take this layer out (Delete)">Remove</button>
          </span>
        {/if}
      </div>
      <p class="muted">{KIND_HELP[layer.t]}</p>
      {#if layer.t === 'hatch'}
        <label class="field">Angle
          <input type="range" min="-90" max="90" step="5" bind:value={layer.angle_deg} disabled={readonly} />
          <input type="number" step="5" bind:value={layer.angle_deg} disabled={readonly} /> °
        </label>
        {#if !readonly}
          <div class="row quick">
            {#each [[0, '— flat'], [45, '/ 45°'], [90, '| upright'], [-45, '\\ −45°']] as [a, label]}
              <button class:on={layer.angle_deg === a} onclick={() => layer.t === 'hatch' && (layer.angle_deg = a as number)}>{label}</button>
            {/each}
          </div>
        {/if}
        <label class="field">Line spacing
          <input type="range" min="0.3" max="12" step="0.1" bind:value={layer.spacing_mm} disabled={readonly} />
          <input type="number" step="0.1" min="0.2" bind:value={layer.spacing_mm} disabled={readonly} /> mm
        </label>
      {:else if layer.t === 'dots'}
        <label class="field">Dot spacing
          <input type="range" min="0.3" max="12" step="0.1" bind:value={layer.spacing_mm} disabled={readonly} />
          <input type="number" step="0.1" min="0.2" bind:value={layer.spacing_mm} disabled={readonly} /> mm
        </label>
        <label class="field">Dot size
          <input type="range" min="0.05" max="2" step="0.05" bind:value={layer.dot_mm} disabled={readonly} />
          <input type="number" step="0.05" min="0.05" bind:value={layer.dot_mm} disabled={readonly} /> mm
        </label>
      {:else if layer.t === 'patlines'}
        <label class="field">Angle
          <input type="range" min="-180" max="180" step="5" bind:value={layer.angle_deg} disabled={readonly} />
          <input type="number" step="1" bind:value={layer.angle_deg} disabled={readonly} /> °
        </label>
        <label class="field" title="Distance from one row to the next">Row spacing
          <input type="range" min="-20" max="20" step="0.1" bind:value={layer.spacing_mm} disabled={readonly} />
          <input type="number" step="0.1" bind:value={layer.spacing_mm} disabled={readonly} /> mm
        </label>
        <label class="field" title="How far along each row starts from the one before (for staggered bricks and steps)">Row shift
          <input type="range" min="-20" max="20" step="0.1" bind:value={layer.shift_mm} disabled={readonly} />
          <input type="number" step="0.1" bind:value={layer.shift_mm} disabled={readonly} /> mm
        </label>
        <label class="field" title="A point the first row passes through">Start point
          <input type="number" step="0.1" bind:value={layer.x_mm} disabled={readonly} aria-label="Start point across" />
          <input type="number" step="0.1" bind:value={layer.y_mm} disabled={readonly} aria-label="Start point up" /> mm
        </label>
        <label class="field" title="Lengths in mm, separated by commas: more than 0 is a dash, less than 0 a gap, 0 a dot. Leave empty for solid lines.">Dashes
          <input class="dashes" type="text" value={dashText(layer.dashes_mm)} onchange={(e) => setDashes(e.currentTarget.value)} disabled={readonly} placeholder="solid, or e.g. 3, -1.5, 0, -1.5" />
        </label>
      {:else}
        <label class="field">Motif
          <select bind:value={layer.motif} disabled={readonly} aria-label="Motif">
            {#each motifNames as n}<option value={n}>{n}</option>{/each}
            {#if !motifNames.includes(layer.motif)}<option value={layer.motif}>{layer.motif} (missing)</option>{/if}
          </select>
        </label>
        <label class="field">Size
          <input type="range" min="0.2" max="10" step="0.1" bind:value={layer.size_mm} disabled={readonly} />
          <input type="number" step="0.1" min="0.2" bind:value={layer.size_mm} disabled={readonly} /> mm
        </label>
        <label class="field">Spacing across
          <input type="range" min="0.5" max="20" step="0.5" bind:value={layer.across_mm} disabled={readonly} />
          <input type="number" step="0.5" min="0.5" bind:value={layer.across_mm} disabled={readonly} /> mm
        </label>
        <label class="field">Spacing down
          <input type="range" min="0.5" max="20" step="0.5" bind:value={layer.down_mm} disabled={readonly} />
          <input type="number" step="0.5" min="0.5" bind:value={layer.down_mm} disabled={readonly} /> mm
        </label>
        <label class="check"><input type="checkbox" bind:checked={layer.stagger} disabled={readonly} /> Shift every other row half a step (like bricks)</label>
        <label class="check"><input type="checkbox" bind:checked={layer.random_turn} disabled={readonly} /> Turn each one a random way</label>
        {#if !layer.random_turn}
          <label class="field">Rotate
            <input type="range" min="-180" max="180" step="5" bind:value={layer.turn_deg} disabled={readonly} />
            <input type="number" step="5" bind:value={layer.turn_deg} disabled={readonly} /> °
          </label>
        {/if}
        <label class="field" title="How much the sizes differ from one to the next">Size variety
          <input type="range" min="0" max="1" step="0.05" bind:value={layer.size_var} disabled={readonly} />
          <span class="val">{pct(layer.size_var)}</span>
        </label>
        <label class="field" title="How far each one wanders from its place in the rows">Scatter
          <input type="range" min="0" max="1" step="0.05" bind:value={layer.jitter} disabled={readonly} />
          <span class="val">{pct(layer.jitter)}</span>
        </label>
        <label class="field" title="Leaves gaps by leaving some out at random">Leave some out
          <input type="range" min="0" max="0.9" step="0.05" bind:value={layer.skip} disabled={readonly} />
          <span class="val">{pct(layer.skip)}</span>
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
        <button onclick={() => ((work = plain(def)), (sel = 0))} disabled={!dirty || unsaved} title="Go back to how it was last saved">Undo changes</button>
        {#if dirty}<span class="unsaved">Not saved yet</span>{/if}
        {#if !unsaved}<button class="danger" onclick={remove}>Delete…</button>{/if}
      </div>
    {/if}
    {#if !dirty}
      <div class="row use">
        <button onclick={drawWith} disabled={!cave.hasCave} title="Pick this fill, then draw round an area">Draw an area with it</button>
        <button onclick={useOnSelected} disabled={!chosenAreas.length} title={chosenAreas.length ? 'Fill the areas selected on the map with this' : 'Select areas on the map first'}>
          Use on {chosenAreas.length ? `${chosenAreas.length} selected area${chosenAreas.length === 1 ? '' : 's'}` : 'selected areas'}
        </button>
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
  .try { display: flex; gap: 4px; align-items: center; color: var(--muted); }
  .try input { width: 90px; }
  .stripbox, .partbox { display: flex; flex-direction: column; gap: 6px; padding: 8px; border: 1px solid var(--line); border-radius: var(--radius); }
  .cards { display: flex; gap: 6px; overflow-x: auto; padding: 2px; }
  .card { flex: none; width: 92px; display: flex; flex-direction: column; gap: 3px; padding: 3px; border: 1px solid var(--line); border-radius: 6px; cursor: pointer; font-size: 11px; text-align: center; }
  .card.sel { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-bg); }
  .card:focus-visible { outline: 2px solid var(--accent); }
  .check { display: flex; gap: 6px; align-items: center; font-size: 12px; color: var(--muted); }
  .add button { font-size: 12px; }
  .quick { padding-left: 116px; }
  .quick button { font-size: 11px; padding: 1px 6px; }
  .quick button.on { background: var(--accent-bg); border-color: var(--accent); }
  .partbox p { margin: 0; }
  .field { display: grid; grid-template-columns: 110px 1fr 64px auto; gap: 6px; align-items: center; font-size: 12px; color: var(--muted); }
  .field select, .field .dashes { grid-column: 2 / 5; }
  .val { font-variant-numeric: tabular-nums; }
  input[type='number'] { width: 64px; }
  input[type='range'] { width: 100%; padding: 0; }
  input[type='checkbox'] { padding: 0; }
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
