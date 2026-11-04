<script lang="ts">
  // The right-click menu (see lib/context-menu.svelte.ts): whatever list was opened, at the pointer, kept on
  // screen. Arrow keys move through it, Enter runs, Escape or a press outside closes it.
  import { tick } from 'svelte';
  import { closeContextMenu, contextMenu, type MenuEntry } from '../lib/context-menu.svelte';

  let box = $state<HTMLDivElement>();
  let w = $state(0);
  let h = $state(0);

  const isAction = (it: MenuEntry): it is Extract<MenuEntry, { run: () => void }> => it !== 'separator' && 'run' in it;

  // the first action takes the keyboard, so arrows and Enter work at once
  $effect(() => {
    if (!contextMenu.open) return;
    void tick().then(() => box?.querySelector<HTMLButtonElement>('button:not([disabled])')?.focus());
  });

  function pick(it: MenuEntry) {
    if (!isAction(it) || it.disabled) return;
    closeContextMenu();
    it.run();
  }

  function keys(e: KeyboardEvent) {
    if (!contextMenu.open) return;
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      closeContextMenu();
      return;
    }
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return;
    e.preventDefault();
    const all = [...(box?.querySelectorAll<HTMLButtonElement>('button:not([disabled])') ?? [])];
    const at = all.indexOf(document.activeElement as HTMLButtonElement);
    const next = e.key === 'ArrowDown' ? (at + 1) % all.length : (at - 1 + all.length) % all.length;
    all[next]?.focus();
  }
</script>

<svelte:window onkeydown={keys} onresize={closeContextMenu} />

{#if contextMenu.open}
  <div class="veil" role="presentation" onpointerdown={closeContextMenu} oncontextmenu={(e) => { e.preventDefault(); closeContextMenu(); }}></div>
  <div
    class="menu"
    role="menu"
    tabindex="-1"
    bind:this={box}
    bind:offsetWidth={w}
    bind:offsetHeight={h}
    style="left: {Math.max(4, Math.min(contextMenu.x, innerWidth - w - 4))}px; top: {Math.max(4, Math.min(contextMenu.y, innerHeight - h - 4))}px"
    oncontextmenu={(e) => e.preventDefault()}
  >
    {#each contextMenu.items as it}
      {#if it === 'separator'}
        <hr />
      {:else if 'heading' in it}
        <div class="heading">{it.heading}</div>
      {:else}
        <button role="menuitem" class:danger={it.danger} disabled={it.disabled} onclick={() => pick(it)}>
          <span>{it.label}</span>{#if it.keys}<kbd>{it.keys}</kbd>{/if}
        </button>
      {/if}
    {/each}
  </div>
{/if}

<style>
  .veil { position: fixed; inset: 0; z-index: 90; }
  .menu { position: fixed; z-index: 91; min-width: 190px; padding: 4px; display: flex; flex-direction: column; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35); }
  .menu button { display: flex; justify-content: space-between; gap: 18px; font: inherit; font-size: 13px; text-align: left; padding: 6px 10px; border: 0; border-radius: 4px; background: none; color: var(--text); cursor: pointer; }
  .menu button:hover, .menu button:focus-visible { background: var(--panel-2); outline: none; }
  .menu button:disabled { opacity: 0.45; cursor: default; }
  .menu button.danger { color: var(--danger); }
  .heading { padding: 4px 10px 2px; font-size: 11px; font-weight: 600; color: var(--muted); }
  kbd { font: inherit; font-size: 11px; color: var(--muted); }
  hr { border: 0; border-top: 1px solid var(--line); margin: 4px 2px; }
</style>
