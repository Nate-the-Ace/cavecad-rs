<script lang="ts">
  // The in-place box for a note's words (see lib/text-edit.svelte.ts).
  import { tick } from 'svelte';
  import { changed } from '../lib/cave.svelte';
  import { engine } from '../lib/engine';
  import { closeTextEdit, isNew, takeMakeNew, textEdit } from '../lib/text-edit.svelte';

  let box = $state<HTMLInputElement>();
  let done = false;

  $effect(() => {
    if (textEdit.open) {
      done = false;
      void tick().then(() => {
        box?.focus();
        box?.select();
      });
    }
  });

  function commit() {
    if (done || !textEdit.open) return;
    done = true;
    if (isNew()) {
      // words that did not exist yet: made only if something was typed
      const make = takeMakeNew();
      const words = textEdit.value.trim();
      closeTextEdit();
      if (make && words) make(words);
      return;
    }
    const r = engine.setLineText(textEdit.id, textEdit.value);
    if (r !== 'ok') engine.log('warn', 'text', r.replace(/^error: /, ''));
    closeTextEdit();
    changed();
  }
  function cancel() {
    done = true;
    closeTextEdit();
  }
</script>

{#if textEdit.open}
  <input
    bind:this={box}
    class="words"
    type="text"
    maxlength="200"
    placeholder={textEdit.placeholder}
    aria-label="Edit the words"
    style="left: {textEdit.x - 6}px; top: {textEdit.y - textEdit.size * 0.75 - 3}px; font-size: {textEdit.size}px; transform: rotate({textEdit.angle}rad); transform-origin: 6px {textEdit.size * 0.75 + 3}px"
    bind:value={textEdit.value}
    onkeydown={(e) => {
      e.stopPropagation();
      if (e.key === 'Enter') commit();
      else if (e.key === 'Escape') cancel();
    }}
    onblur={commit}
    onpointerdown={(e) => e.stopPropagation()}
  />
{/if}

<style>
  .words { position: fixed; z-index: 95; min-width: 140px; box-sizing: content-box; padding: 2px 6px; font-family: system-ui, sans-serif; color: var(--text); background: var(--panel); border: 1.5px solid var(--accent); border-radius: 4px; outline: none; }
</style>
