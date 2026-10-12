<script lang="ts">
  // A length entry field. Every tool uses this for anything a person types as a distance, so the
  // View tab's unit setting decides what the numbers mean. The value it holds is always metres.
  import { cave } from '../lib/cave.svelte';
  import { lengthText, parseLength, unitName } from '../lib/units';

  let { value = $bindable<number | null>(null), id, label = '', min }: { value?: number | null; id: string; label?: string; min?: number } = $props();

  let text = $state('');
  let bad = $state(false);
  let editing = false;

  // Show the held value in the current unit, unless the person is in the middle of typing.
  $effect(() => {
    const u = cave.units;
    const v = value;
    if (!editing) {
      text = v === null ? '' : lengthText(v, u);
      bad = false;
    }
  });

  function commit() {
    editing = false;
    if (text.trim() === '') {
      value = null;
      bad = false;
      return;
    }
    const m = parseLength(text, cave.units);
    if (m === null || (min !== undefined && m < min)) {
      bad = true;
      return;
    }
    bad = false;
    value = m;
    text = lengthText(m, cave.units);
  }
</script>

<span class="field" class:bad>
  {#if label}<label for={id}>{label}</label>{/if}
  <input {id} inputmode="decimal" autocomplete="off" bind:value={text} oninput={() => (editing = true)} onblur={commit} onkeydown={(e) => e.key === 'Enter' && commit()} aria-invalid={bad} />
  <span class="unit">{unitName(cave.units)}</span>
  {#if bad}<span class="msg" role="alert">Not a length{cave.units === 'imperial' ? " (try 12.5 or 5'6\")" : ' (try 12.5 or 30 cm)'}</span>{/if}
</span>

<style>
  .field { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; }
  input { width: 7em; font: inherit; padding: 3px 6px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); color: var(--text); text-align: right; }
  .bad input { border-color: var(--danger); }
  .unit { color: var(--muted); }
  .msg { color: var(--danger); }
</style>
