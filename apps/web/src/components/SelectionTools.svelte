<script lang="ts">
  // What can be done to the selected line: change what it is, close or open its curve. (Delete and
  // Done sit in the tab's Command group.) Each is one undoable edit in the view the line was drawn in.
  import { onMount } from 'svelte';
  import { Spline, FlipHorizontal2 } from '@lucide/svelte';
  import { cave, changed, option, requestRefit } from '../lib/cave.svelte';
  import { engine, loadEngine, type TypeStyle } from '../lib/engine';
  import { isStampKind } from '../lib/symbols';
  import { fillDefs, isFillKind } from '../lib/fills';
  import { ARROW_LABELS, FRAME_LABELS, LINETYPE_LABELS, PATTERN_PREFIX, SHAPE_LABELS, liveStyle, parseStyle, styleText, type LineStyle } from '../lib/line-style.svelte';

  let tools = $state<TypeStyle[]>([]);
  onMount(() => {
    loadEngine().then(() => (tools = engine.drawTools()));
  });
  const shaped = ['ledge', 'pit', 'flowstone', 'rimstone', 'slope', 'dripline', 'glyphs'];

  const info = $derived.by(() => {
    void cave.version;
    return cave.selected ? engine.lineInfo(cave.selected) : null;
  });
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

  // This line's own look: its pattern, how long its dashes are, and (for an area) how dense its hatch is. Dragging a slider
  // shows the result at once in every view; letting go saves it as one undoable edit.
  const style = $derived(parseStyle(liveStyle.id === cave.selected ? liveStyle.style : info?.style));
  const draft = (patch: Partial<LineStyle>) => {
    liveStyle.id = cave.selected;
    liveStyle.style = styleText({ ...style, ...patch });
  };
  function keep() {
    if (liveStyle.id !== cave.selected) return;
    const text = liveStyle.style;
    run(engine.setLineStyle(cave.selected, text));
    liveStyle.id = '';
  }
  const resetStyle = () => {
    liveStyle.id = '';
    run(engine.setLineStyle(cave.selected, ''));
  };
  // a different line, or none: nothing is half-way
  $effect(() => {
    if (liveStyle.id && liveStyle.id !== cave.selected) liveStyle.id = '';
  });

  /** The note switches with `letter` turned on or off, as the letters stored in the style. */
  const toggle = (letter: string, on: boolean) => [...'fhn'].filter((c) => (c === letter ? on : style.flags.includes(c))).join('');
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
    <span>Pattern angle <b>{style.angle.toFixed(0)}°</b></span>
    <input type="range" min="-90" max="90" step="1" value={style.angle} oninput={(e) => draft({ angle: Number(e.currentTarget.value) })} onchange={keep} aria-label="Pattern angle" />
  </label>
{/snippet}

<div class="sel">
  {#if isFill && info}
    <div class="curve">
      <select class="kind" value={info.kind} aria-label="What the area is" onchange={(e) => run(engine.setLineKind(cave.selected, e.currentTarget.value))}>
        {#each fillDefs() as d}<option value={d.key}>{d.label}</option>{/each}
        <option value="fill-custom">My fill</option>
      </select>
      {#if isCustomFill}
        <select class="kind" value={style.lt} aria-label="Which of your fills" title="The fill, from the Patterns panel" onchange={(e) => (draft({ lt: e.currentTarget.value }), keep())}>
          {#if !style.lt.startsWith(PATTERN_PREFIX)}<option value="">Pick a fill…</option>{/if}
          {#each patterns.fills as f}<option value={PATTERN_PREFIX + f.name}>{f.name}</option>{/each}
        </select>
      {/if}
      {#if isCustomFill}
        <label class="detail" title="How many marks or hatch lines this area has">
          <span>Hatch density <b>×{style.density.toFixed(2)}</b></span>
          <input type="range" min="0.25" max="4" step="0.05" value={style.density} oninput={(e) => draft({ density: Number(e.currentTarget.value) })} onchange={keep} aria-label="Hatch density" />
        </label>
        {@render angleSlider()}
      {/if}
      <span class="tip">Drag a point to reshape the area: it refills.</span>
    </div>
  {:else if !isSymbol}
  <div class="types">
    <select class="kind" value={info?.kind ?? ''} aria-label="What this line is" title="Change what this line is" onchange={(e) => info && run(engine.setLineKind(cave.selected, e.currentTarget.value))}>
      {#each tools.filter((t) => t.key !== 'text') as t}<option value={t.key}>{t.label}</option>{/each}
    </select>
  </div>
  {/if}
  <div class="curve">
    {#if !isSymbol && !isFill}
    <button class="tool" class:pressed={info?.closed} aria-pressed={info?.closed} title={info?.closed ? 'Open the curve: it stops at its last point' : 'Close the curve: it runs from its last point back to its first (needs three points)'} onclick={() => info && run(engine.setLineClosed(cave.selected, !info.closed))}>
      <Spline size={22} strokeWidth={1.6} />
      <span>{info?.closed ? 'Open' : 'Close'}</span>
    </button>
    {/if}
    {#if !isSymbol && info}
      <label class="detail" title="Fit the line again with more or fewer points. Fewer points are easier to edit; more follow the curve closely.">
        <span>Curve detail</span>
        <input type="range" min="1" max="10" step="1" value={option('detail', 5)} onchange={(e) => requestRefit(cave.selected, Number(e.currentTarget.value))} aria-label="Curve detail" />
      </label>
    {/if}
    {#if isSymbol && info}
      <input class="words" type="text" value={info.text} placeholder={info.kind === 'text' ? 'The text' : info.kind === 'callout' ? 'The note' : 'Label'} maxlength="200" aria-label={info.kind === 'text' ? 'The text' : info.kind === 'callout' ? 'The note' : 'Label'} onchange={(e) => run(engine.setLineText(cave.selected, e.currentTarget.value))} />
      <span class="tip">Drag the dot to move it, the square to turn and resize it.</span>
    {/if}
    {#if info && shaped.includes(info.kind)}
      <button class="tool" class:pressed={info.flip} aria-pressed={info.flip} title="Put the marks on the other side of the line" onclick={() => run(engine.setLineFlip(cave.selected, !info.flip))}>
        <FlipHorizontal2 size={22} strokeWidth={1.6} />
        <span>Flip side</span>
      </button>
    {/if}
    {#if message}<span class="msg" role="status">{message}</span>{/if}
  </div>
  {#if info && isLeader}
    <div class="style" role="group" aria-label="Leader">
      <select class="kind" value={style.arrow} aria-label="Arrowhead" title="The end of the leader at the thing it points to" onchange={(e) => (draft({ arrow: e.currentTarget.value }), keep())}>
        {#each ARROW_LABELS as [key, label]}<option value={key}>{label}</option>{/each}
      </select>
      <select class="kind" value={style.shape} aria-label="Leader line" title="How the leader runs from the arrow to the note" onchange={(e) => (draft({ shape: e.currentTarget.value }), keep())}>
        {#each SHAPE_LABELS as [key, label]}<option value={key}>{label}</option>{/each}
      </select>
      <select class="kind" value={style.frame} aria-label="Note frame" title="A box or an underline round the note" onchange={(e) => (draft({ frame: e.currentTarget.value }), keep())}>
        {#each FRAME_LABELS as [key, label]}<option value={key}>{label}</option>{/each}
      </select>
      <label class="check" title="Draw the line round the note's frame"><input type="checkbox" checked={!style.flags.includes('n')} disabled={!style.frame || style.frame === 'underline'} onchange={(e) => (draft({ flags: toggle('n', !e.currentTarget.checked) }), keep())} /> Frame outline</label>
      <label class="check" title="A paper-coloured backing behind the note, so lines under it do not show through"><input type="checkbox" checked={style.flags.includes('f')} disabled={!style.frame || style.frame === 'underline'} onchange={(e) => (draft({ flags: toggle('f', e.currentTarget.checked) }), keep())} /> Fill</label>
      <label class="check" title="An outline round the letters, so the text reads over lines and fills"><input type="checkbox" checked={style.flags.includes('h')} onchange={(e) => (draft({ flags: toggle('h', e.currentTarget.checked) }), keep())} /> Text outline</label>
      <button class="small" title="Then press where the new arrow should touch (in any view)" class:on={cave.arrowOp.id === cave.selected && cave.arrowOp.op === 'add'} onclick={() => arm('add')}>Add arrow</button>
      <button class="small" title="Then press the arrow to take off (a leader keeps at least one)" class:on={cave.arrowOp.id === cave.selected && cave.arrowOp.op === 'remove'} onclick={() => arm('remove')}>Remove arrow</button>
      <label class="detail" title="How big the note's text is: ×1 is 3 mm on paper">
        <span>Text size <b>×{style.size.toFixed(2)}</b></span>
        <input type="range" min="0.5" max="3" step="0.05" value={style.size} oninput={(e) => draft({ size: Number(e.currentTarget.value) })} onchange={keep} aria-label="Leader text size" />
      </label>
      <button class="small" onclick={resetStyle} disabled={!info.style && liveStyle.id !== cave.selected}>Reset</button>
    </div>
  {:else if info && !isSymbol && !isCustomFill}
    <div class="style" role="group" aria-label="Line style">
      <select class="kind" value={style.lt} aria-label="Linetype" title="The pattern of this line: its type's own, or another" onchange={(e) => (draft({ lt: e.currentTarget.value }), keep())}>
        {#each LINETYPE_LABELS as [key, label]}<option value={key}>{label}</option>{/each}
        {#if patterns.lines.length}
          <optgroup label="Patterns">
            {#each patterns.lines as l}<option value={PATTERN_PREFIX + l.name}>{l.name}</option>{/each}
          </optgroup>
        {/if}
      </select>
      <label class="detail" title="How long this line's dashes and gaps are, whatever its pattern">
        <span>Linetype scale <b>×{style.scale.toFixed(2)}</b></span>
        <input type="range" min="0.25" max="4" step="0.05" value={style.scale} oninput={(e) => draft({ scale: Number(e.currentTarget.value) })} onchange={keep} aria-label="Linetype scale" />
      </label>
      {#if isFill}
        <label class="detail" title="How many marks or hatch lines this area has">
          <span>Hatch density <b>×{style.density.toFixed(2)}</b></span>
          <input type="range" min="0.25" max="4" step="0.05" value={style.density} oninput={(e) => draft({ density: Number(e.currentTarget.value) })} onchange={keep} aria-label="Hatch density" />
        </label>
        {@render angleSlider()}
      {/if}
      <button class="small" onclick={resetStyle} disabled={!info.style && liveStyle.id !== cave.selected} title="Back to what its type gives it">Reset</button>
    </div>
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
  .tip { font-size: 11px; color: var(--muted); max-width: 150px; line-height: 1.25; }
  .msg { font-size: 11px; color: var(--danger); max-width: 130px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
