<script lang="ts">
  // The Eraser. Pick it, then drag over linework in the Map (Top view), Profile or Cross Section: the part of a line
  // under the circle is cut away, and a fill, symbol, text or leader it touches goes whole. Pick it again, or press
  // Escape, to put it down. A pen's eraser end does the same without picking anything.
  import { Eraser } from '@lucide/svelte';
  import { cave, setTool } from '../lib/cave.svelte';

  const on = $derived(cave.tool === 'eraser');
</script>

<div class="tools" role="group" aria-label="Erase">
  <button class="tool" class:pressed={on} aria-pressed={on} title={on ? 'Put the Eraser down (Esc)' : 'Eraser: drag over lines in the Map, Profile or Cross Section to cut them away'} onclick={() => setTool('eraser')}>
    <Eraser size={22} strokeWidth={1.6} />
    <span>Eraser</span>
  </button>
</div>

<style>
  .tools { display: flex; align-items: center; gap: 4px; height: 100%; }
  .tool { display: flex; flex-direction: column; align-items: center; gap: 4px; min-width: 58px; box-sizing: border-box; height: calc(var(--ribbon-body) - 1px); justify-content: center; overflow: hidden; padding: 4px 8px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; color: var(--text); }
  .tool:hover { background: var(--panel-2); }
  .tool.pressed { background: var(--accent-bg); outline: 1px solid var(--accent); }
</style>
