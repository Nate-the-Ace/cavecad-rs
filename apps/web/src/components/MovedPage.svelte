<script lang="ts">
  // Shown when CaveCAD is opened from an old live, class or join link (`?live=`, `?class=`, `?join=`, or
  // `cavecad://join` in the desktop app). Those were retired on 2026-10-26: caves now live on the CaveCAD
  // server and are shared through rooms. The button tidies the address and opens the Cave Shelf.
  import { shelf } from '../lib/shelf.svelte';
  import { withoutOldLink } from '../lib/moved-link';

  let { open = $bindable(false) }: { open?: boolean } = $props();

  function toShelf() {
    history.replaceState(null, '', withoutOldLink(location.pathname, location.search, location.hash));
    open = false;
    shelf.open = true;
  }
</script>

{#if open}
  <div class="veil" role="presentation">
    <div class="box" role="dialog" aria-modal="true" aria-labelledby="moved-title" tabindex="-1" onkeydown={(e) => e.key === 'Escape' && toShelf()}>
      <h2 id="moved-title">Live sessions moved to rooms</h2>
      <p>Open your caves from the Cave Shelf.</p>
      <div class="row">
        <button class="go" onclick={toShelf}>Open the Cave Shelf</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .veil { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.5); display: grid; place-items: center; z-index: 50; }
  .box { width: min(420px, 92vw); background: var(--panel); color: var(--text); border: 1px solid var(--line); border-radius: 10px; padding: 18px 20px; display: flex; flex-direction: column; gap: 12px; outline: none; }
  h2 { margin: 0; font-size: 17px; }
  p { margin: 0; font-size: 13px; }
  .row { display: flex; justify-content: flex-end; gap: 8px; }
  button { font: inherit; font-size: 13px; padding: 6px 14px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); cursor: pointer; }
  button.go { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
