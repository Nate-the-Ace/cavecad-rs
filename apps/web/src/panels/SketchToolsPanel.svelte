<script lang="ts">
  // The sketch tools as a small dockable panel of their own: place it beside whichever pane you draw on.
  import { sketch, type Tool } from '../lib/sketch.svelte';

  const TOOLS: { id: Tool; label: string; hint: string }[] = [
    { id: 'wall', label: 'Wall', hint: 'A solid line: passage wall' },
    { id: 'ledge', label: 'Ledge', hint: 'A dashed line: ledge or edge' },
    { id: 'note', label: 'Note', hint: 'A line in the accent colour: anything to remember' },
    { id: 'erase', label: 'Erase', hint: "Drag over a line to remove it (the pen's eraser end does this too)" },
    { id: 'pan', label: 'Move', hint: 'Drag to move the view (two fingers always do)' },
  ];
</script>

<div class="tools" role="toolbar" aria-label="Sketch tools">
  {#each TOOLS as t}
    <button class:sel={sketch.tool === t.id} aria-pressed={sketch.tool === t.id} title={t.hint} onclick={() => (sketch.tool = t.id)}>{t.label}</button>
  {/each}
  <span class="sp"></span>
  <label class="chk" title="Show the station you are standing at in the cross section, and keep the views on it"><input type="checkbox" bind:checked={sketch.followSurvey} /> Follow the survey</label>
</div>

<style>
  .tools { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; padding: 6px 8px; height: 100%; box-sizing: border-box; background: var(--panel); color: var(--text); font-size: 13px; align-content: flex-start; }
  button { font: inherit; padding: 8px 14px; min-height: 40px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); cursor: pointer; touch-action: manipulation; }
  button.sel { background: var(--accent); border-color: var(--accent); color: var(--on-accent, #fff); }
  .sp { flex: 1; }
  .chk { display: inline-flex; align-items: center; gap: 6px; color: var(--muted); min-height: 40px; }
  .chk input { width: 18px; height: 18px; }
</style>
