<script lang="ts">
  // What can be done to the selected line: change what it is, close or open its curve. (Delete and
  // Done sit in the tab's Command group.) Each is one undoable edit in the view the line was drawn in.
  import { onMount } from 'svelte';
  import { BrickWall, Minus, CircleDashed, Pencil, Route, Spline } from '@lucide/svelte';
  import { cave, changed } from '../lib/cave.svelte';
  import { engine, loadEngine, type TypeStyle } from '../lib/engine';

  let tools = $state<TypeStyle[]>([]);
  onMount(() => {
    loadEngine().then(() => (tools = engine.drawTools()));
  });
  const icons: Record<string, typeof Route> = { wall: BrickWall, ledge: Minus, pit: CircleDashed, sketch: Pencil };

  const info = $derived.by(() => {
    void cave.version;
    return cave.selected ? engine.lineInfo(cave.selected) : null;
  });
  let message = $state('');

  const run = (r: string) => {
    message = r === 'ok' ? '' : r.replace(/^error: /, '');
    changed();
  };
</script>

<div class="sel">
  <div class="types" role="group" aria-label="What this line is">
    {#each tools as t}
      {@const Icon = icons[t.key] ?? Route}
      <button class="tool" class:pressed={info?.kind === t.key} aria-pressed={info?.kind === t.key} title="Make this line a {t.label}" onclick={() => info && run(engine.setLineKind(cave.selected, t.key))}>
        <Icon size={22} strokeWidth={1.6} />
        <span>{t.label}</span>
      </button>
    {/each}
  </div>
  <div class="curve">
    <button class="tool" class:pressed={info?.closed} aria-pressed={info?.closed} title={info?.closed ? 'Open the curve: it stops at its last point' : 'Close the curve: it runs from its last point back to its first (needs three points)'} onclick={() => info && run(engine.setLineClosed(cave.selected, !info.closed))}>
      <Spline size={22} strokeWidth={1.6} />
      <span>{info?.closed ? 'Open' : 'Close'}</span>
    </button>
    {#if message}<span class="msg" role="status">{message}</span>{/if}
  </div>
</div>

<style>
  .sel { display: flex; align-items: center; gap: 14px; height: 100%; }
  .types { display: flex; gap: 4px; }
  .curve { display: flex; flex-direction: column; align-items: center; gap: 2px; }
  .tool { display: flex; flex-direction: column; align-items: center; gap: 4px; min-width: 58px; box-sizing: border-box; height: calc(var(--ribbon-body) - 1px); justify-content: center; overflow: hidden; padding: 4px 8px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; color: var(--text); }
  .tool:hover { background: var(--panel-2); }
  .tool.pressed { background: var(--accent-bg); outline: 1px solid var(--accent); }
  .msg { font-size: 11px; color: var(--danger); max-width: 130px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
</style>
