<script lang="ts">
  // Saved layouts, as a group in the ribbon's View tab: six slots. Click a full slot to bring its arrangement of
  // panels back (and the map lock it was saved with); click an empty one to pick it, then name it and Save to keep
  // what is on screen there. Two short rows, to fit the ribbon's fixed height.
  import { engine } from '../lib/engine';
  import { LAYOUT_SLOTS, slotName } from '../lib/layout-slots';
  import { clearSlot, layouts, recallSlot, saveSlot } from '../lib/layouts.svelte';

  const slots = $derived(layouts.slots);
  const i = $derived(layouts.selected);
  const kept = $derived(slots[i] ?? null);

  // The name box shows the chosen slot's name, and is the name Save will use.
  let name = $state('');
  $effect(() => {
    name = slots[layouts.selected]?.name ?? '';
  });

  const tell = (m: string) => m && engine.log('info', 'layout', m);

  function pick(n: number) {
    layouts.selected = n;
    if (slots[n]) tell(recallSlot(n));
  }
  const save = () => tell(saveSlot(i, name));
</script>

<div class="slots" role="group" aria-label="Saved layouts">
  <div class="row">
    {#each { length: LAYOUT_SLOTS } as _, n}
      <button
        class="slot"
        class:full={!!slots[n]}
        class:sel={i === n}
        aria-pressed={i === n}
        title={slots[n] ? `${slotName(slots, n)}: bring this layout back` : `Slot ${n + 1} is empty: pick it, then Save`}
        onclick={() => pick(n)}>{n + 1}</button>
    {/each}
  </div>
  <div class="row">
    <input class="name" bind:value={name} maxlength="30" placeholder={`Layout ${i + 1}`} aria-label="Name of the layout in slot {i + 1}" onkeydown={(e) => e.key === 'Enter' && save()} />
    <button class="mini" title="Keep the panels as they are now in slot {i + 1}" onclick={save}>Save</button>
    <button class="mini" disabled={!kept} title="Empty slot {i + 1}" onclick={() => clearSlot(i)}>Clear</button>
  </div>
</div>

<style>
  .slots { display: flex; flex-direction: column; justify-content: center; gap: 5px; height: 100%; font-size: 12px; }
  .row { display: flex; gap: 4px; align-items: center; }
  .slot { width: 30px; height: 26px; padding: 0; border: 1px solid var(--line); border-radius: var(--radius); background: none; color: var(--muted); cursor: pointer; font: inherit; position: relative; }
  .slot.full { color: var(--text); background: var(--panel-2); }
  .slot.full::after { content: ''; position: absolute; right: 3px; bottom: 3px; width: 5px; height: 5px; border-radius: 50%; background: var(--accent); }
  .slot.sel { border-color: var(--accent); background: var(--accent-bg); color: var(--text); }
  .name { flex: 1; min-width: 0; width: 94px; height: 22px; box-sizing: border-box; padding: 0 6px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); font: inherit; }
  .mini { height: 22px; padding: 0 8px; border: 1px solid var(--line); border-radius: var(--radius); background: none; color: var(--text); cursor: pointer; font: inherit; }
  .mini:hover:not(:disabled) { background: var(--panel-2); }
  .mini:disabled { opacity: 0.4; cursor: default; }
</style>
