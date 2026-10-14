<script lang="ts">
  // The right-click menu on a drawn line: Select Similar, Delete.
  import { cave, changed, selectIds, selectedIds } from '../lib/cave.svelte';
  import { engine } from '../lib/engine';
  import { closeLineMenu, lineMenu } from '../lib/line-menu.svelte';

  function similar() {
    selectIds(lineMenu.similar());
    closeLineMenu();
  }
  function remove() {
    const ids = selectedIds();
    cave.selected = '';
    for (const id of ids) engine.deleteLine(id);
    closeLineMenu();
    changed();
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && lineMenu.open && closeLineMenu()} />

{#if lineMenu.open}
  <div class="veil" role="presentation" onpointerdown={closeLineMenu} oncontextmenu={(e) => { e.preventDefault(); closeLineMenu(); }}></div>
  <div class="menu" role="menu" style="left: {Math.min(lineMenu.x, innerWidth - 190)}px; top: {Math.min(lineMenu.y, innerHeight - 90)}px">
    <button role="menuitem" onclick={similar}>Select Similar</button>
    <button role="menuitem" onclick={remove}>Delete</button>
  </div>
{/if}

<style>
  .veil { position: fixed; inset: 0; z-index: 90; }
  .menu { position: fixed; z-index: 91; min-width: 170px; padding: 4px; display: flex; flex-direction: column; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35); }
  .menu button { font: inherit; font-size: 13px; text-align: left; padding: 6px 10px; border: 0; border-radius: 4px; background: none; color: var(--text); cursor: pointer; }
  .menu button:hover { background: var(--panel-2); }
</style>
