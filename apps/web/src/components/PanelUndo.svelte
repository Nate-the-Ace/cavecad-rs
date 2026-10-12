<script lang="ts">
  // Undo and Redo for one panel. Every panel that can be drawn on (or otherwise edited) puts this in
  // its toolbar with its own `scope`, so it only ever undoes what was done in that panel. The same
  // keys work while the panel has focus: use `undoKey` from the panel's keydown handler.
  import { untrack } from 'svelte';
  import { Undo2, Redo2 } from '@lucide/svelte';
  import { cave, changed } from '../lib/cave.svelte';
  import { engine, loadEngine, type Scope } from '../lib/engine';

  let { scope, label = 'here' }: { scope: Scope; label?: string } = $props();
  let can = $state({ undo: false, redo: false });
  let ready = $state(false);
  loadEngine().then(() => (ready = true));

  $effect(() => {
    void cave.version;
    void ready;
    untrack(() => {
      can = ready ? engine.editState(scope) : { undo: false, redo: false };
    });
  });

  const undo = () => engine.undo(scope) && changed();
  const redo = () => engine.redo(scope) && changed();
</script>

<button title="Undo {label} (Ctrl/Cmd+Z)" aria-label="Undo {label}" disabled={!can.undo} onclick={undo}><Undo2 size={15} /></button>
<button title="Redo {label} (Shift+Ctrl/Cmd+Z)" aria-label="Redo {label}" disabled={!can.redo} onclick={redo}><Redo2 size={15} /></button>
