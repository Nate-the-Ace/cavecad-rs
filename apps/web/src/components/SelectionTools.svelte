<script lang="ts">
  // What can be done to the selected line: change what it is, close or open its curve. (Delete and
  // Done sit in the tab's Command group.) Each is one undoable edit in the view the line was drawn in.
  import { onMount } from 'svelte';
  import { Spline, FlipHorizontal2 } from '@lucide/svelte';
  import { cave, changed } from '../lib/cave.svelte';
  import { engine, loadEngine, type TypeStyle } from '../lib/engine';
  import { isStampKind } from '../lib/symbols';
  import { fillDefs, isFillKind } from '../lib/fills';

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
    {#if isSymbol && info}
      <input class="words" type="text" value={info.text} placeholder={info.kind === 'text' ? 'The text' : 'Label'} maxlength="200" aria-label={info.kind === 'text' ? 'The text' : 'Label'} onchange={(e) => run(engine.setLineText(cave.selected, e.currentTarget.value))} />
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
  .tip { font-size: 11px; color: var(--muted); max-width: 150px; line-height: 1.25; }
  .msg { font-size: 11px; color: var(--danger); max-width: 130px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
