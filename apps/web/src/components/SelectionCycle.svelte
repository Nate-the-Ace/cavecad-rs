<script lang="ts">
  // The selection cycling list (lib/cycle.svelte.ts): beside the pointer after a click on stacked things, the topmost lit.
  // Passing over a row selects it for a look; clicking it keeps it. Arrow keys or Tab move, Enter keeps, Escape or a
  // press anywhere else closes (keeping what is selected). The view under it keeps the keyboard, so Delete still works.
  import { cave } from '../lib/cave.svelte';
  import { acceptRow, backToChosen, closeCycle, cycle, lightRow, setCycling } from '../lib/cycle.svelte';

  let box = $state<HTMLDivElement>();
  let w = $state(0);
  let h = $state(0);

  // an edit (an undo, a delete) closes it: a row could name something that is gone
  $effect(() => {
    if (cycle.open && cave.version !== cycle.version) closeCycle();
  });

  function keys(e: KeyboardEvent) {
    if (!cycle.open) return;
    const n = cycle.items.length;
    const step = e.key === 'ArrowDown' || (e.key === 'Tab' && !e.shiftKey) ? 1 : e.key === 'ArrowUp' || (e.key === 'Tab' && e.shiftKey) ? -1 : 0;
    if (step) {
      e.preventDefault();
      e.stopPropagation();
      lightRow((cycle.index + step + n) % n);
      cycle.chosen = cycle.index; // moved by keys: this is the choice now
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      e.stopPropagation();
      acceptRow();
      return;
    }
    if (e.key === 'Escape') {
      e.preventDefault();
      e.stopPropagation();
      closeCycle();
      return;
    }
    if (/^[1-9]$/.test(e.key) && Number(e.key) <= n) {
      e.preventDefault();
      e.stopPropagation();
      lightRow(Number(e.key) - 1, true);
      closeCycle();
    }
  }

  // a press anywhere else closes it; that press does its own work as usual (a new click on the map may open a new list)
  function away(e: PointerEvent) {
    if (cycle.open && box && !box.contains(e.target as Node)) closeCycle();
  }
</script>

<svelte:window onkeydowncapture={keys} onpointerdowncapture={away} onresize={closeCycle} />

{#if cycle.open}
  <div
    class="cycle"
    role="listbox"
    aria-label="Things under the click"
    tabindex="-1"
    bind:this={box}
    bind:offsetWidth={w}
    bind:offsetHeight={h}
    style="left: {Math.max(4, Math.min(cycle.x + 14, innerWidth - w - 4))}px; top: {Math.max(4, Math.min(cycle.y + 10, innerHeight - h - 4))}px"
    onpointerleave={backToChosen}
    oncontextmenu={(e) => e.preventDefault()}
  >
    <div class="head"><span>{cycle.items.length} here</span><button class="off" title="Stop listing stacked things after a click (switch it back on from a right click on empty space in a view)" onclick={() => setCycling(false)}>Turn off</button></div>
    {#each cycle.items as it, i (it.key)}
      <button
        role="option"
        aria-selected={i === cycle.index}
        class:lit={i === cycle.index}
        onpointerenter={() => lightRow(i)}
        onclick={() => {
          lightRow(i, true);
          closeCycle();
        }}
      >
        <kbd>{i + 1}</kbd><span class="label">{it.label}</span>{#if it.detail}<span class="detail">{it.detail}</span>{/if}
      </button>
    {/each}
  </div>
{/if}

<style>
  .cycle { position: fixed; z-index: 89; min-width: 180px; max-width: 320px; max-height: min(360px, calc(100vh - 8px)); overflow: auto; padding: 4px; display: flex; flex-direction: column; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); box-shadow: 0 6px 20px rgba(0, 0, 0, 0.35); font-size: 13px; color: var(--text); }
  .head { display: flex; justify-content: space-between; align-items: center; gap: 12px; padding: 2px 6px 4px 10px; font-size: 11px; font-weight: 600; color: var(--muted); }
  .off { font: inherit; font-weight: 400; font-size: 11px; padding: 1px 4px; border: 0; background: none; color: var(--muted); cursor: pointer; text-decoration: underline; }
  .off:hover { color: var(--text); }
  button[role='option'] { display: flex; align-items: baseline; gap: 8px; font: inherit; text-align: left; padding: 5px 10px 5px 6px; border: 0; border-radius: 4px; background: none; color: var(--text); cursor: pointer; }
  button[role='option'].lit { background: var(--panel-2); }
  kbd { font: inherit; font-size: 11px; color: var(--muted); min-width: 1em; text-align: right; }
  .label { flex: 0 0 auto; }
  .detail { flex: 1 1 auto; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; color: var(--muted); }
</style>
