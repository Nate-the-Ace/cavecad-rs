<script lang="ts">
  // The Survey tab's shot settings, as a group in the ribbon: whether the surveyor or the instrument says what
  // a shot is (Leg or Splay, for an instrument that does not), whether a backsight follows each leg, and whether
  // a shot is written at once or waits for Accept. Fits the ribbon's fixed body height: a row for the segmented
  // choice and a row of checkboxes, like the options of a running command.
  import * as live from '../lib/survey.svelte';
  import { survey } from '../lib/survey.svelte';
</script>

<div class="opts">
  {#if survey.groupsLegs}
    <p class="note" title="Three or more consecutive close shots are a leg; any other shot is a splay. An X1 also flags legs itself, and can mark a leg Backsight with its M button; the other instruments cannot, so a leg that comes back along the one before is taken as its backsight.">3 close shots are a leg, 1 is a splay</p>
  {:else}
    <div class="seg" role="group" aria-label="What the next shot is">
      <button class:sel={survey.state.mode === 'leg'} aria-pressed={survey.state.mode === 'leg'} onclick={() => live.setMode('leg')}>Leg</button>
      <button class:sel={survey.state.mode === 'splay'} aria-pressed={survey.state.mode === 'splay'} onclick={() => live.setMode('splay')}>Splay</button>
    </div>
  {/if}
  <div class="row">
    <label class="opt" title="Take a backsight after each leg, from the new station looking back">
      <input type="checkbox" checked={survey.state.backsights} onchange={(e) => live.setBacksights(e.currentTarget.checked)} />
      <span>Backsights</span>
    </label>
    <label class="opt" title="Off: each shot waits for Accept in the Survey panel before it is written">
      <input type="checkbox" bind:checked={survey.auto} />
      <span>Write at once</span>
    </label>
  </div>
</div>

<style>
  .opts { display: flex; flex-direction: column; gap: 6px; justify-content: center; height: 100%; min-width: 190px; font-size: 12px; overflow: hidden; }
  .note { margin: 0; height: 24px; line-height: 24px; color: var(--muted); white-space: nowrap; }
  .seg { display: inline-flex; height: 24px; }
  .seg button { min-width: 64px; padding: 0 12px; border: 1px solid var(--line); background: var(--panel-2); color: var(--text); cursor: pointer; font: inherit; }
  .seg button:first-child { border-radius: var(--radius) 0 0 var(--radius); }
  .seg button:last-child { border-radius: 0 var(--radius) var(--radius) 0; border-left: none; }
  .seg button.sel { background: var(--accent); border-color: var(--accent); color: var(--on-accent, #fff); }
  .row { display: flex; gap: 14px; }
  .opt { display: flex; align-items: center; gap: 6px; height: 22px; color: var(--text); white-space: nowrap; }
</style>
