<script lang="ts">
  // Starting a cave from nothing: its name and the unit the notes are written in. Everything else is
  // built up from the scans, shot by shot, in the Notebook.
  import { cave, setUnits } from '../lib/cave.svelte';
  import { engine } from '../lib/engine';
  import { startCave } from '../lib/files';
  import { workLayout } from '../lib/workspace.svelte';
  import { shelf } from '../lib/shelf.svelte';

  let name = $state('');
  let unit = $state<'ft' | 'm'>(cave.units === 'imperial' ? 'ft' : 'm');
  let message = $state('');
  let input = $state<HTMLInputElement>();

  $effect(() => {
    if (cave.newDialog) queueMicrotask(() => input?.focus());
  });

  function create() {
    const r = startCave(name, unit);
    if (r === null) return; // declined to leave the cave on screen
    engine.log(r.ok ? 'info' : 'error', 'file', r.message);
    if (!r.ok) {
      message = r.message;
      return;
    }
    // What people read and type follows what the notes are written in, so numbers go in as written.
    setUnits(unit === 'ft' ? 'imperial' : 'metric');
    cave.newDialog = false;
    shelf.open = false;
    name = '';
    workLayout(); // scans and notebook side by side, ready to start typing
  }

  const close = () => (cave.newDialog = false);
</script>

{#if cave.newDialog}
  <div class="veil" role="presentation" onclick={close}>
    <div class="box" role="dialog" aria-modal="true" aria-label="New cave" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === 'Escape' && close()}>
      <h2>New cave</h2>
      <label>Name
        <input bind:this={input} bind:value={name} placeholder="Cave name" maxlength="80" onkeydown={(e) => e.key === 'Enter' && create()} />
      </label>
      <fieldset>
        <legend>The notes are written in</legend>
        <label><input type="radio" bind:group={unit} value="ft" /> Feet</label>
        <label><input type="radio" bind:group={unit} value="m" /> Metres</label>
      </fieldset>
      <p class="hint">You will get your scans on the left and the notebook beside them. Open the folder your scans are in, pick the first page, and type its shots as you read them.</p>
      {#if message}<p class="err" role="alert">{message}</p>{/if}
      <div class="row">
        <button onclick={close}>Cancel</button>
        <button class="go" onclick={create}>Start the cave</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .veil { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.5); display: grid; place-items: center; z-index: 50; }
  .box { width: min(420px, 92vw); background: var(--panel); color: var(--text); border: 1px solid var(--line); border-radius: 10px; padding: 18px 20px; display: flex; flex-direction: column; gap: 12px; outline: none; }
  h2 { margin: 0; font-size: 17px; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--muted); }
  fieldset { border: 1px solid var(--line); border-radius: 8px; display: flex; gap: 16px; padding: 6px 10px 8px; font-size: 13px; }
  fieldset label { flex-direction: row; align-items: center; gap: 6px; color: var(--text); }
  legend { font-size: 12px; color: var(--muted); padding: 0 4px; }
  input:not([type='radio']) { font: inherit; font-size: 14px; padding: 6px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); }
  .hint { margin: 0; font-size: 12px; color: var(--muted); }
  .err { margin: 0; color: var(--danger); font-size: 12px; }
  .row { display: flex; justify-content: flex-end; gap: 8px; }
  button { font: inherit; font-size: 13px; padding: 6px 14px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); cursor: pointer; }
  button.go { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
