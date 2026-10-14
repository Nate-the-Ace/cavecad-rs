<script lang="ts">
  // What can be done to the selected line: change what it is, close or open its curve. (Delete and
  // Done sit in the tab's Command group.) Each is one undoable edit in the view the line was drawn in.
  import { onMount } from 'svelte';
  import { Spline, FlipHorizontal2 } from '@lucide/svelte';
  import { cave, changed, option, requestRefit, selectedIds } from '../lib/cave.svelte';
  import { engine, loadEngine, type TypeStyle } from '../lib/engine';
  import { isStampKind } from '../lib/symbols';
  import { fillDefs, isFillKind } from '../lib/fills';
  import { copyOp } from '../lib/view-clip.svelte';
  import { ALIGN_LABELS, ARROW_LABELS, FONT_LABELS, FRAME_LABELS, LINETYPE_LABELS, PATTERN_PREFIX, SHAPE_LABELS, liveStyle, parseStyle, styleText, withFlag, type LineStyle } from '../lib/line-style.svelte';

  let tools = $state<TypeStyle[]>([]);
  onMount(() => {
    loadEngine().then(() => (tools = engine.drawTools()));
  });
  const shaped = ['ledge', 'pit', 'flowstone', 'rimstone', 'slope', 'dripline', 'glyphs'];

  // One or several selected lines. Several of one kind are edited together: a setting they share shows its value,
  // one they do not shows VARIOUS, and changing it changes it for every one of them. Several of different kinds
  // get only the basics.
  const ids = $derived.by(() => {
    void cave.also;
    return selectedIds();
  });
  const infos = $derived.by(() => {
    void cave.version;
    return ids.map((id) => engine.lineInfo(id)).filter((i): i is NonNullable<ReturnType<typeof engine.lineInfo>> => !!i);
  });
  const info = $derived(infos[0] ?? null);
  const multi = $derived(ids.length > 1);
  const alike = $derived(infos.every((i) => i.kind === infos[0]?.kind));
  const isSymbol = $derived(!!info && isStampKind(info.kind));
  const isFill = $derived(!!info && isFillKind(info.kind));
  const isLeader = $derived(info?.kind === 'callout');
  const isCustomFill = $derived(info?.kind === 'fill-custom');
  /** The patterns the caver can pick for a line or an area: the built-in ones and the cave's own. */
  const patterns = $derived.by(() => {
    void cave.version;
    return engine.patternLibrary();
  });
  let message = $state('');
  const copying = $derived(copyOp.on);
  /** The selection has words whose format can be set (a text, a leader, or a symbol with a label). */
  const hasWords = $derived(infos.some((i) => i.kind === 'text' || i.kind === 'callout' || !!i.text));

  // This line's own look: its pattern, how long its dashes are, and (for an area) how dense its hatch is. Dragging a slider
  // shows the result at once in every view; letting go saves it as one undoable edit.
  // With several lines selected, the settings changed so far (applied to each line's own style when let go).
  let pending = $state<Partial<LineStyle>>({});
  const styles = $derived(infos.map((i) => parseStyle(i.style)));
  const style = $derived(multi ? { ...(styles[0] ?? parseStyle('')), ...pending } : parseStyle(liveStyle.id === cave.selected ? liveStyle.style : info?.style));
  /** True when the selected lines do not share a value for this setting. */
  const various = (field: keyof LineStyle) => multi && !(field in pending) && new Set(styles.map((s) => String(s[field]))).size > 1;
  const draft = (patch: Partial<LineStyle>) => {
    if (multi) {
      pending = { ...pending, ...patch };
      return;
    }
    liveStyle.id = cave.selected;
    liveStyle.style = styleText({ ...style, ...patch });
  };
  function keep() {
    if (multi) {
      const patch = pending;
      pending = {};
      let r = 'ok';
      infos.forEach((i, n) => {
        const out = engine.setLineStyle(ids[n], styleText({ ...parseStyle(i.style), ...patch }));
        if (out !== 'ok') r = out;
      });
      run(r);
      return;
    }
    if (liveStyle.id !== cave.selected) return;
    const text = liveStyle.style;
    run(engine.setLineStyle(cave.selected, text));
    liveStyle.id = '';
  }
  const resetStyle = () => {
    liveStyle.id = '';
    pending = {};
    let r = 'ok';
    for (const id of ids) {
      const out = engine.setLineStyle(id, '');
      if (out !== 'ok') r = out;
    }
    run(r);
  };
  // a different selection: nothing is half-way
  $effect(() => {
    void ids.join(',');
    pending = {};
  });
  /** Applies a change to every selected line. */
  const each = (f: (id: string) => string) => {
    let r = 'ok';
    for (const id of ids) {
      const out = f(id);
      if (out !== 'ok') r = out;
    }
    run(r);
  };
  // a different line, or none: nothing is half-way
  $effect(() => {
    if (liveStyle.id && liveStyle.id !== cave.selected) liveStyle.id = '';
  });

  /** Whether every selected note has this switch on. */
  const hasFlag = (letter: string) => (multi ? styles.every((s) => s.flags.includes(letter)) : style.flags.includes(letter));
  /** Switches a note setting (bold, italic, underline, fill, ...) on or off for every selected line, keeping each one's other switches. */
  function setFlag(letter: string, on: boolean) {
    if (!multi) {
      draft({ flags: withFlag(style.flags, letter, on) });
      keep();
      return;
    }
    let r = 'ok';
    ids.forEach((id, n) => {
      const out = engine.setLineStyle(id, styleText({ ...styles[n], flags: withFlag(styles[n].flags, letter, on) }));
      if (out !== 'ok') r = out;
    });
    run(r);
  }
  /** Asks for the next press in a view to add or remove an arrow of this leader (pressing the same button again cancels). */
  const arm = (op: 'add' | 'remove') => {
    cave.arrowOp = cave.arrowOp.id === cave.selected && cave.arrowOp.op === op ? { id: '', op: '' } : { id: cave.selected, op };
  };
  // a different leader, or none: nothing is waiting for a press
  $effect(() => {
    if (cave.arrowOp.id && cave.arrowOp.id !== cave.selected) cave.arrowOp = { id: '', op: '' };
  });

  const run = (r: string) => {
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    changed();
  };
</script>

{#snippet angleSlider()}
  <label class="detail" title="Turns the pattern inside the area; the outline stays where it is">
    <span>Pattern angle <b>{various('angle') ? 'VARIOUS' : `${style.angle.toFixed(0)}°`}</b></span>
    <input type="range" min="-90" max="90" step="1" value={style.angle} oninput={(e) => draft({ angle: Number(e.currentTarget.value) })} onchange={keep} aria-label="Pattern angle" />
  </label>
{/snippet}

{#snippet textFormat()}
    <div class="style" role="group" aria-label="Text format">
      <select class="kind" value={various('font') ? '' : style.font} aria-label="Type face" title="The type face of the words" onchange={(e) => (draft({ font: e.currentTarget.value }), keep())}>
        {#if various('font')}<option value="" disabled selected>VARIOUS</option>{/if}
        {#each FONT_LABELS as [key, label]}<option value={key}>{label}</option>{/each}
      </select>
      <label class="detail" title="How big the words are, as a multiple of the usual">
        <span>Size <b>{various('size') ? 'VARIOUS' : `×${style.size.toFixed(2)}`}</b></span>
        <input type="range" min="0.4" max="4" step="0.05" value={style.size} oninput={(e) => draft({ size: Number(e.currentTarget.value) })} onchange={keep} aria-label="Text size" />
      </label>
      <div class="seg" role="group" aria-label="Style">
        <button class:pressed={hasFlag('b')} aria-pressed={hasFlag('b')} title="Bold" onclick={() => setFlag('b', !hasFlag('b'))}><b>B</b></button>
        <button class:pressed={hasFlag('i')} aria-pressed={hasFlag('i')} title="Italic" onclick={() => setFlag('i', !hasFlag('i'))}><i>I</i></button>
        <button class:pressed={hasFlag('u')} aria-pressed={hasFlag('u')} title="Underline" onclick={() => setFlag('u', !hasFlag('u'))}><u>U</u></button>
      </div>
      <div class="seg" role="group" aria-label="Alignment">
        {#each ALIGN_LABELS as [key, label]}
          <button class:pressed={!various('align') && style.align === key} aria-pressed={!various('align') && style.align === key} title="{label}: where the words sit against their point" onclick={() => (draft({ align: key }), keep())}>{label[0]}</button>
        {/each}
      </div>
      <label class="check" title="The colour of the words (Auto uses the line's own ink)">
        <input type="color" value={various('color') || !style.color ? '#e8e6e1' : style.color} aria-label="Text colour" onchange={(e) => (draft({ color: e.currentTarget.value }), keep())} />
        <button class="small" disabled={!various('color') && !style.color} onclick={(e) => (e.preventDefault(), draft({ color: '' }), keep())}>Auto</button>
      </label>
    </div>
{/snippet}

<div class="sel">
  {#if copying}
    <span class="tip copy" role="status">{copyOp.base ? 'Click to drop copies' : 'Pick a base point'}</span>
  {/if}
  {#if multi && !alike}
    <div class="curve">
      <span class="tip many">{ids.length} items of different kinds are selected.</span>
      <button class="small" onclick={resetStyle} title="Back to what each one's type gives it">Reset style</button>
      {#if message}<span class="msg" role="status">{message}</span>{/if}
    </div>
    {#if hasWords && infos.every((i) => isStampKind(i.kind))}{@render textFormat()}{/if}
  {:else}
  {#if isFill && info}
    <div class="curve">
      <select class="kind" value={info.kind} aria-label="What the area is" onchange={(e) => each((id) => engine.setLineKind(id, e.currentTarget.value))}>
        {#each fillDefs() as d}<option value={d.key}>{d.label}</option>{/each}
        <option value="fill-custom">My fill</option>
      </select>
      {#if isCustomFill}
        <select class="kind" value={style.lt} aria-label="Which of your fills" title="The fill, from the Patterns panel" onchange={(e) => (draft({ lt: e.currentTarget.value }), keep())}>
          {#if various('lt')}<option value="" disabled selected>VARIOUS</option>{:else if !style.lt.startsWith(PATTERN_PREFIX)}<option value="">Pick a fill…</option>{/if}
          {#each patterns.fills as f}<option value={PATTERN_PREFIX + f.name}>{f.name}</option>{/each}
        </select>
      {/if}
      {#if isCustomFill}
        <label class="detail" title="How many marks or hatch lines this area has">
          <span>Hatch density <b>{various('density') ? 'VARIOUS' : `×${style.density.toFixed(2)}`}</b></span>
          <input type="range" min="0.25" max="4" step="0.05" value={style.density} oninput={(e) => draft({ density: Number(e.currentTarget.value) })} onchange={keep} aria-label="Hatch density" />
        </label>
        {@render angleSlider()}
      {/if}
      <span class="tip">Drag a point to reshape the area: it refills.</span>
    </div>
  {:else if !isSymbol}
  <div class="types">
    <select class="kind" value={info?.kind ?? ''} aria-label="What this line is" title="Change what this line is" onchange={(e) => info && each((id) => engine.setLineKind(id, e.currentTarget.value))}>
      {#each tools.filter((t) => t.key !== 'text') as t}<option value={t.key}>{t.label}</option>{/each}
    </select>
  </div>
  {/if}
  <div class="curve">
    {#if !isSymbol && !isFill}
    <button class="tool" class:pressed={infos.every((i) => i.closed)} aria-pressed={infos.every((i) => i.closed)} title={info?.closed ? 'Open the curve: it stops at its last point' : 'Close the curve: it runs from its last point back to its first (needs three points)'} onclick={() => info && each((id) => engine.setLineClosed(id, !infos.every((i) => i.closed)))}>
      <Spline size={22} strokeWidth={1.6} />
      <span>{infos.every((i) => i.closed) ? 'Open' : 'Close'}</span>
    </button>
    {/if}
    {#if !isSymbol && info && !multi}
      <label class="detail" title="Fit the line again with more or fewer points. Fewer points are easier to edit; more follow the curve closely.">
        <span>Curve detail</span>
        <input type="range" min="1" max="10" step="1" value={option('detail', 5)} onchange={(e) => requestRefit(cave.selected, Number(e.currentTarget.value))} aria-label="Curve detail" />
      </label>
    {/if}
    {#if isSymbol && info}
      <input class="words" title="The words. Drag the dot to move it, the square to turn and resize it." type="text" value={new Set(infos.map((i) => i.text)).size > 1 ? '' : info.text} placeholder={new Set(infos.map((i) => i.text)).size > 1 ? 'VARIOUS' : info.kind === 'text' ? 'The text' : info.kind === 'callout' ? 'The note' : 'Label'} maxlength="200" aria-label={info.kind === 'text' ? 'The text' : info.kind === 'callout' ? 'The note' : 'Label'} onchange={(e) => each((id) => engine.setLineText(id, e.currentTarget.value))} />
      
    {/if}
    {#if info && shaped.includes(info.kind)}
      <button class="tool" class:pressed={infos.every((i) => i.flip)} aria-pressed={infos.every((i) => i.flip)} title="Put the marks on the other side of the line" onclick={() => each((id) => engine.setLineFlip(id, !infos.every((i) => i.flip)))}>
        <FlipHorizontal2 size={22} strokeWidth={1.6} />
        <span>Flip side</span>
      </button>
    {/if}
    {#if message}<span class="msg" role="status">{message}</span>{/if}
  </div>
  {#if info && isSymbol && hasWords}
    {@render textFormat()}
  {/if}
  {#if info && isLeader}
    <div class="style" role="group" aria-label="Leader">
      <select class="kind" value={various('arrow') ? '' : style.arrow} aria-label="Arrowhead" title="The end of the leader at the thing it points to" onchange={(e) => (draft({ arrow: e.currentTarget.value }), keep())}>
        {#if various('arrow')}<option value="" disabled selected>VARIOUS</option>{/if}{#each ARROW_LABELS as [key, label]}<option value={key}>{label}</option>{/each}
      </select>
      <select class="kind" value={various('shape') ? '' : style.shape} aria-label="Leader line" title="How the leader runs from the arrow to the note" onchange={(e) => (draft({ shape: e.currentTarget.value }), keep())}>
        {#if various('shape')}<option value="" disabled selected>VARIOUS</option>{/if}{#each SHAPE_LABELS as [key, label]}<option value={key}>{label}</option>{/each}
      </select>
      <select class="kind" value={various('frame') ? '' : style.frame} aria-label="Note frame" title="A box or an underline round the note" onchange={(e) => (draft({ frame: e.currentTarget.value }), keep())}>
        {#if various('frame')}<option value="" disabled selected>VARIOUS</option>{/if}{#each FRAME_LABELS as [key, label]}<option value={key}>{label}</option>{/each}
      </select>
      <label class="check" title="Draw the line round the note's frame"><input type="checkbox" indeterminate={various('flags')} checked={!style.flags.includes('n')} disabled={!style.frame || style.frame === 'underline'} onchange={(e) => setFlag('n', !e.currentTarget.checked)} /> Frame outline</label>
      <label class="check" title="A paper-coloured backing behind the note, so lines under it do not show through"><input type="checkbox" indeterminate={various('flags')} checked={style.flags.includes('f')} disabled={!style.frame || style.frame === 'underline'} onchange={(e) => setFlag('f', e.currentTarget.checked)} /> Fill</label>
      <label class="check" title="An outline round the letters, so the text reads over lines and fills"><input type="checkbox" indeterminate={various('flags')} checked={style.flags.includes('h')} onchange={(e) => setFlag('h', e.currentTarget.checked)} /> Text outline</label>
      {#if !multi}
        <button class="small" title="Then press where the new arrow should touch (in any view)" class:on={cave.arrowOp.id === cave.selected && cave.arrowOp.op === 'add'} onclick={() => arm('add')}>Add arrow</button>
        <button class="small" title="Then press the arrow to take off (a leader keeps at least one)" class:on={cave.arrowOp.id === cave.selected && cave.arrowOp.op === 'remove'} onclick={() => arm('remove')}>Remove arrow</button>
      {/if}
      <button class="small" onclick={resetStyle} disabled={!infos.some((i) => i.style) && liveStyle.id !== cave.selected}>Reset</button>
    </div>
  {:else if info && !isSymbol && !isCustomFill}
    <div class="style" role="group" aria-label="Line style">
      <select class="kind" value={various('lt') ? '' : style.lt} aria-label="Linetype" title="The pattern of this line: its type's own, or another" onchange={(e) => (draft({ lt: e.currentTarget.value }), keep())}>
        {#if various('lt')}<option value="" disabled selected>VARIOUS</option>{/if}
        {#each LINETYPE_LABELS as [key, label]}<option value={key}>{label}</option>{/each}
        {#if patterns.lines.length}
          <optgroup label="Patterns">
            {#each patterns.lines as l}<option value={PATTERN_PREFIX + l.name}>{l.name}</option>{/each}
          </optgroup>
        {/if}
      </select>
      <label class="detail" title="How long this line's dashes and gaps are, whatever its pattern">
        <span>Linetype scale <b>{various('scale') ? 'VARIOUS' : `×${style.scale.toFixed(2)}`}</b></span>
        <input type="range" min="0.25" max="4" step="0.05" value={style.scale} oninput={(e) => draft({ scale: Number(e.currentTarget.value) })} onchange={keep} aria-label="Linetype scale" />
      </label>
      {#if isFill}
        <label class="detail" title="How many marks or hatch lines this area has">
          <span>Hatch density <b>{various('density') ? 'VARIOUS' : `×${style.density.toFixed(2)}`}</b></span>
          <input type="range" min="0.25" max="4" step="0.05" value={style.density} oninput={(e) => draft({ density: Number(e.currentTarget.value) })} onchange={keep} aria-label="Hatch density" />
        </label>
        {@render angleSlider()}
      {/if}
      <button class="small" onclick={resetStyle} disabled={!infos.some((i) => i.style) && liveStyle.id !== cave.selected} title="Back to what its type gives it">Reset</button>
    </div>
  {/if}
  {/if}
</div>

<style>
  .sel { display: flex; align-items: center; gap: 14px; height: 100%; }
  .types { display: flex; gap: 4px; }
  .curve { display: flex; align-items: center; gap: 4px; }
  .tool { display: flex; flex-direction: column; align-items: center; gap: 4px; min-width: 58px; box-sizing: border-box; height: calc(var(--ribbon-body) - 1px); justify-content: center; overflow: hidden; padding: 4px 8px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; color: var(--text); }
  .tool:hover { background: var(--panel-2); }
  .tool.pressed { background: var(--accent-bg); outline: 1px solid var(--accent); }
  .words { width: 170px; height: 26px; box-sizing: border-box; padding: 0 8px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); font: inherit; font-size: 13px; }
  .kind { font: inherit; font-size: 13px; height: 26px; padding: 0 6px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); }
  .detail { display: flex; flex-direction: column; gap: 2px; font-size: 11px; color: var(--muted); justify-content: center; height: calc(var(--ribbon-body) - 1px); }
  .detail input { width: 110px; }
  .style { display: flex; align-items: center; gap: 8px; }
  .style b { color: var(--text); font-weight: 600; }
  .small { font: inherit; font-size: 12px; padding: 3px 9px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); cursor: pointer; }
  .small.on { background: var(--accent); color: #fff; border-color: var(--accent); }
  .check { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; color: var(--text); }
  .small:disabled { opacity: 0.5; cursor: default; }
  .many { max-width: 200px; }
  .seg { display: inline-flex; border: 1px solid var(--line); border-radius: 6px; overflow: hidden; }
  .seg button { font: inherit; font-size: 13px; min-width: 26px; height: 26px; padding: 0 6px; border: 0; background: var(--panel-2); color: var(--text); cursor: pointer; }
  .seg button + button { border-left: 1px solid var(--line); }
  .seg button.pressed { background: var(--accent); color: #fff; }
  input[type='color'] { width: 30px; height: 26px; padding: 0; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); cursor: pointer; }
  .copy { max-width: 90px; color: var(--accent); font-weight: 600; }
  .tip { font-size: 11px; color: var(--muted); max-width: 150px; line-height: 1.25; }
  .msg { font-size: 11px; color: var(--danger); max-width: 130px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
