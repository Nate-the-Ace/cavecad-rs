<script lang="ts">
  // "What's new": shown once after an update (the entries this device has not seen), or on asking (all of them).
  import { X } from '@lucide/svelte';
  import { CHANGES, markChangesSeen, type Change } from '../lib/changelog';

  let { changes, onclose }: { changes: Change[]; onclose: () => void } = $props();
  const shown = $derived(changes.length > 0 ? changes : CHANGES);
  function close() {
    markChangesSeen();
    onclose();
  }
</script>

<div class="veil" role="presentation" onclick={close}>
  <div class="box" role="dialog" aria-modal="true" aria-label="What's new in CaveCAD" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === 'Escape' && close()}>
    <header>
      <b>What's new in CaveCAD</b>
      <button onclick={close} aria-label="Close"><X size={20} /> Close</button>
    </header>
    <div class="page">
      {#each shown as c (c.id)}
        <section>
          <h2>{c.title}</h2>
          <p class="date">{c.date}</p>
          <ul>{#each c.items as item}<li>{item}</li>{/each}</ul>
        </section>
      {/each}
    </div>
    <footer><button class="go" onclick={close}>Got it</button></footer>
  </div>
</div>

<style>
  .veil { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); display: grid; place-items: center; z-index: 80; }
  .box { width: min(640px, 94vw); max-height: min(86vh, 860px); background: var(--panel); color: var(--text); border: 1px solid var(--line); border-radius: 10px; display: flex; flex-direction: column; outline: none; overflow: hidden; }
  header, footer { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 8px 14px; }
  header { border-bottom: 1px solid var(--line); }
  footer { border-top: 1px solid var(--line); justify-content: flex-end; }
  button { display: inline-flex; align-items: center; gap: 4px; font: inherit; min-height: 40px; padding: 4px 14px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); cursor: pointer; }
  button.go { background: var(--accent); border-color: var(--accent); color: #fff; }
  .page { overflow: auto; padding: 6px 18px 14px; font-size: 14px; line-height: 1.45; }
  h2 { font-size: 16px; margin: 12px 0 0; }
  .date { margin: 2px 0 8px; color: var(--muted); font-size: 12px; }
  ul { margin: 0; padding-left: 1.2em; }
  li { margin: 0 0 6px; }
  @media (max-width: 600px), (max-height: 500px) {
    .box { width: 100vw; max-height: none; height: 100dvh; border-radius: 0; border: 0; padding-top: env(safe-area-inset-top); padding-bottom: env(safe-area-inset-bottom); box-sizing: border-box; }
  }
</style>
