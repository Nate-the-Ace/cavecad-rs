<script lang="ts">
  // What can be done to the selected line: change what it is, close or open its curve. (Delete and
  // Done sit in the tab's Command group.) Each is one undoable edit in the view the line was drawn in.
  import { onMount } from 'svelte';
  import { Spline, FlipHorizontal2 } from '@lucide/svelte';
  import { cave, changed, option, requestRefit } from '../lib/cave.svelte';
  import { engine, loadEngine, type TypeStyle } from '../lib/engine';
  import { isStampKind } from '../lib/symbols';
  import { fillDefs, isFillKind } from '../lib/fills';
  import { LINETYPE_LABELS, liveStyle, parseStyle, styleText, type LineStyle } from '../lib/line-style.svelte';

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

  const run = (r: string) => {
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    changed();
  };
</script>

<div class="sel">
  {#if isFill && info}
    <div class="curve">
      <select class="kind" value={info.kind} aria-label="What the area is" onchange={(e) => run(engine.setLineKind(cave.selected, e.currentTarget.value))}>
        {#each fillDefs() as d}<option value={d.key}>{d.label}</option>{/each}
      </select>
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
  {#if info && !isSymbol}
    <div class="style" role="group" aria-label="Line style">
      <select class="kind" value={style.lt} aria-label="Linetype" title="The pattern of this line: its type's own, or another" onchange={(e) => (draft({ lt: e.currentTarget.value }), keep())}>
        {#each LINETYPE_LABELS as [key, label]}<option value={key}>{label}</option>{/each}
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
  .small:disabled { opacity: 0.5; cursor: default; }
  .tip { font-size: 11px; color: var(--muted); max-width: 150px; line-height: 1.25; }
  .msg { font-size: 11px; color: var(--danger); max-width: 130px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
