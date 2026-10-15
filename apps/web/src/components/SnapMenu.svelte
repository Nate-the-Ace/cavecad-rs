<script lang="ts">
  // The one-pick snap menu (Shift or Ctrl right click while a tool waits for a point): choosing an entry applies to the
  // next pick only. A web menu of our own, each entry showing the marker it draws. Escape or a click elsewhere closes it.
  import { snap, onePick, freePick } from '../lib/snap.svelte';
  import { glyph } from '../lib/snap-draw';
  import type { SnapMode } from '../lib/snap-types';

  const groups = $derived([
    { name: 'Reference', modes: snap.catalog.filter((m) => m.group === 'reference') },
    { name: 'Point', modes: snap.catalog.filter((m) => m.group === 'point') },
    { name: 'Relational', modes: snap.catalog.filter((m) => m.group === 'relational') },
  ]);

  /** Draws an entry's marker onto its canvas. */
  function mark(node: HTMLCanvasElement, mode: SnapMode) {
    const g = node.getContext('2d');
    if (!g) return;
    g.clearRect(0, 0, 20, 20);
    glyph(g, mode, 10, 10, 5.5, getComputedStyle(node).color);
  }

  // The context-menu event that follows the right click that opened the menu lands on the shade: it must not close it.
  let openedAt = 0;
  $effect(() => {
    if (snap.menu) openedAt = performance.now();
  });
  const away = () => {
    if (performance.now() - openedAt > 250) snap.menu = null;
  };

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.stopPropagation();
      snap.menu = null;
    }
  }
</script>

<svelte:window onkeydowncapture={key} />

{#if snap.menu}
  <button class="shade" aria-label="Close the snap menu" onpointerdown={away} oncontextmenu={(e) => e.preventDefault()}></button>
  <div class="menu" role="menu" aria-label="Snap for the next point" style="left:{Math.min(snap.menu.x, window.innerWidth - 220)}px; top:{Math.min(snap.menu.y, window.innerHeight - 440)}px">
    {#each groups as g}
      <div class="head">{g.name}</div>
      {#each g.modes as m (m.id)}
        <button role="menuitem" class:on={snap.session.override === m.id} onclick={() => onePick(m.id)}>
          <canvas width="20" height="20" use:mark={m.id}></canvas>{m.name}
        </button>
      {/each}
    {/each}
    <div class="head">Other</div>
    <button role="menuitem" onclick={freePick}><canvas width="20" height="20"></canvas>None (one free point)</button>
    <button role="menuitem" onclick={() => ((snap.menu = null), (snap.settingsOpen = true))}><canvas width="20" height="20"></canvas>Snap settings…</button>
  </div>
{/if}

<style>
  .shade { position: fixed; inset: 0; background: transparent; border: none; padding: 0; z-index: 40; cursor: default; }
  .menu { position: fixed; z-index: 41; width: 210px; max-height: calc(100vh - 20px); overflow: auto; padding: 4px; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35); font-size: 12px; color: var(--text); }
  .head { padding: 6px 8px 2px; color: var(--muted); font-size: 10px; text-transform: uppercase; letter-spacing: 0.06em; }
  button[role='menuitem'] { display: flex; align-items: center; gap: 8px; width: 100%; padding: 3px 8px; border: none; background: transparent; color: var(--text); text-align: left; cursor: pointer; font: inherit; border-radius: 3px; }
  button[role='menuitem']:hover, button[role='menuitem'].on { background: var(--panel-2); }
  button.on { color: var(--accent); }
  canvas { width: 20px; height: 20px; }
</style>
