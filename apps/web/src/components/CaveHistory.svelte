<script lang="ts">
  // Opens an EARLIER version of a cave kept on the server, as a separate copy. The server keeps every edit ever made to a
  // cave, in order; this lists the latest of them in a few words each, and opening one of them builds a copy of the cave
  // with that edit and everything before it, and nothing after. The cave itself and its history are never touched: the
  // copy gets a room of its own, so it never syncs back into the original.
  import { onMount } from 'svelte';
  import { openEarlierCopy, readHistory, type CaveHistory } from '../lib/caves';
  import { shelf } from '../lib/shelf.svelte';

  let { roomId, name, onclose }: { roomId: string; name: string; onclose: () => void } = $props();

  let history = $state<CaveHistory | null>(null);
  let problem = $state('');
  let busy = $state(false);
  /** How many of the entries after the snapshot the copy keeps: -1 until one is picked. */
  let keep = $state(-1);

  const SHOWN = 80;
  onMount(async () => {
    const r = await readHistory(roomId);
    if (typeof r === 'string') problem = r;
    else history = r;
  });

  /** The newest entries, newest first, with how many entries come before each. */
  const rows = $derived(history ? history.entries.map((e, i) => ({ ...e, index: i })).slice(-SHOWN).reverse() : []);
  const hidden = $derived(history ? Math.max(0, history.entries.length - SHOWN) : 0);

  async function open() {
    if (keep < 0 || busy) return;
    busy = true;
    const r = await openEarlierCopy(roomId, keep);
    busy = false;
    if (r) problem = r;
    else {
      shelf.open = false;
      onclose();
    }
  }
</script>

<div class="veil" role="presentation"></div>
<div class="hist" role="dialog" aria-modal="true" aria-label="Open an earlier version of {name}">
  <h3>An earlier version of {name}</h3>
  <p class="note">Pick the last edit you want to keep. The copy has that edit and everything before it, and nothing after. {name} itself is not changed.</p>
  {#if problem}<p class="err" role="alert">{problem}</p>{/if}
  {#if !history && !problem}<p class="note">Reading its history…</p>{/if}
  {#if history}
    <div class="list" role="listbox" aria-label="Edits, newest first">
      {#each rows as r (r.number)}
        <button role="option" aria-selected={keep === r.index + 1} class:on={keep === r.index + 1} onclick={() => (keep = r.index + 1)}>
          <span class="n">#{r.number}</span> <span class="w">{r.what}</span>
        </button>
      {/each}
      <button role="option" aria-selected={keep === 0} class:on={keep === 0} onclick={() => (keep = 0)}>
        <span class="n">before all of these</span> <span class="w">{hidden ? `(${hidden} older edits are not listed)` : 'the cave as the server last saved it whole'}</span>
      </button>
    </div>
  {/if}
  <div class="foot">
    <button onclick={onclose} disabled={busy}>Cancel</button>
    <button class="go" onclick={open} disabled={keep < 0 || busy || !history}>{busy ? 'Opening…' : 'Open that version as a copy'}</button>
  </div>
</div>

<style>
  .veil { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); z-index: 60; }
  .hist { position: fixed; z-index: 61; left: 8px; right: 8px; top: 8vh; bottom: 8vh; display: flex; flex-direction: column; gap: 6px; padding: 12px; background: var(--panel); color: var(--text); border: 1px solid var(--line); border-radius: 10px; max-width: 560px; margin: 0 auto; }
  h3 { margin: 0; font-size: 15px; }
  .note { margin: 0; color: var(--muted); font-size: 12px; }
  .err { margin: 0; color: var(--danger); font-size: 12px; }
  .list { flex: 1 1 0; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 2px; }
  .list button { display: flex; gap: 8px; align-items: baseline; text-align: left; padding: 8px; border: 1px solid transparent; border-radius: 6px; background: var(--panel-2); color: var(--text); font: inherit; font-size: 12px; min-height: 40px; touch-action: manipulation; }
  .list button.on { background: var(--accent); border-color: var(--accent); color: var(--on-accent, #fff); }
  .n { flex: none; color: var(--muted); min-width: 4.5em; }
  .on .n { color: inherit; }
  .w { min-width: 0; overflow-wrap: anywhere; }
  .foot { display: flex; justify-content: flex-end; gap: 8px; }
  .foot button { min-height: 40px; padding: 0 14px; border: 1px solid var(--line); border-radius: 8px; background: var(--panel-2); color: var(--text); font: inherit; }
  .foot .go { background: var(--accent); border-color: var(--accent); color: var(--on-accent, #fff); font-weight: 600; }
  .foot button:disabled { opacity: 0.5; }
</style>
