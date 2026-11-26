<script lang="ts">
  // Opens an EARLIER version of a cave kept on the server, as a separate copy. The server keeps every edit ever made to a
  // cave, in order; this lists the latest of them in a few words each, and opening one of them builds a copy of the cave
  // with that edit and everything before it, and nothing after. The cave itself and its history are never touched: the
  // copy gets a room of its own, so it never syncs back into the original.
  import { onMount } from 'svelte';
  import { slide } from 'svelte/transition';
  import CavePicture from './CavePicture.svelte';
  import { openEarlierCopy, previewVersion, readHistory, restoreOpenCave, versionOps, type CaveHistory } from '../lib/caves';
  import { engine } from '../lib/engine';
  import { cave } from '../lib/cave.svelte';
  import { formatLength } from '../lib/units';
  import { shelf } from '../lib/shelf.svelte';

  let { roomId, name, onclose }: { roomId: string; name: string; onclose: () => void } = $props();

  let history = $state<CaveHistory | null>(null);
  let problem = $state('');
  let busy = $state(false);
  /** How many of the entries after the snapshot the copy keeps: -1 until one is picked. */
  let keep = $state(-1);
  /** This cave is the one on screen: it can be put back in place, keeping its history. */
  const isOpen = $derived(cave.hasCave && engine.roomId() === roomId);
  const preview = $derived(history && keep >= 0 ? previewVersion(history, keep) : null);
  /** The picked version drawn in plan: it opens under its row. */
  const picture = $derived(history && keep >= 0 ? engine.previewOfOps(versionOps(history, keep)) : null);
  let done = $state('');

  const SHOWN = 400;
  /** What the cave measured after each edit (length, depth), worked out in the background so the list can show where it changed. */
  let after = $state<Record<number, string>>({});
  $effect(() => {
    const h = history;
    if (!h) return;
    let stop = false;
    (async () => {
      const out: Record<number, string> = {};
      for (let i = h.entries.length - 1; i >= Math.max(0, h.entries.length - SHOWN) && !stop; i--) {
        const sm = previewVersion(h, i + 1);
        out[h.entries[i].number] = !sm || sm.empty ? 'empty' : `${formatLength(sm.length_m, cave.units, 0)}${sm.depth_m === null ? '' : ` · ${formatLength(sm.depth_m, cave.units, 0)} deep`}`;
        if (i % 8 === 0) {
          after = { ...out };
          await new Promise((r) => setTimeout(r, 0));
        }
      }
      if (!stop) after = { ...out };
    })();
    return () => (stop = true);
  });
  onMount(async () => {
    const r = await readHistory(roomId);
    if (typeof r === 'string') problem = r;
    else history = r;
  });

  /** The newest entries, newest first, with how many entries come before each. */
  const rows = $derived(history ? history.entries.map((e, i) => ({ ...e, index: i })).slice(-SHOWN).reverse() : []);
  const hidden = $derived(history ? Math.max(0, history.entries.length - SHOWN) : 0);

  /** Puts the open cave back to the picked version, as a new edit: its history stays and the restore can be undone. */
  function restore() {
    if (keep < 0 || !history || busy) return;
    const r = restoreOpenCave(history, roomId, keep);
    if (r.startsWith('ok:')) done = `Restored: ${r.slice(3)} change${r.slice(3) === '1' ? '' : 's'}. The cave's history is kept, and Undo on the Map takes the restore back.`;
    else problem = r;
  }

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
  <p class="note">Pick the last edit you want to keep: the version has that edit and everything before it, and nothing after. Restoring puts {name} back to it as one more step, so its history stays; a copy is a separate cave and {name} is not touched.</p>
  {#if problem}<p class="err" role="alert">{problem}</p>{/if}
  {#if !history && !problem}<p class="note">Reading its history…</p>{/if}
  {#if history}
    <div class="list" role="group" aria-label="Edits, newest first">
      {#each rows as r (r.number)}
        <div class="entry" class:on={keep === r.index + 1}>
          <button aria-pressed={keep === r.index + 1} aria-expanded={keep === r.index + 1} class:on={keep === r.index + 1} onclick={() => (keep = keep === r.index + 1 ? -1 : r.index + 1)}>
            <span class="n">#{r.number}</span> <span class="w">{r.what}</span> <span class="m">{after[r.number] ?? ''}</span>
          </button>
          {#if keep === r.index + 1}
            <div class="open" transition:slide={{ duration: 200 }}><CavePicture {picture} /></div>
          {/if}
        </div>
      {/each}
      <div class="entry" class:on={keep === 0}>
        <button aria-pressed={keep === 0} aria-expanded={keep === 0} class:on={keep === 0} onclick={() => (keep = keep === 0 ? -1 : 0)}>
          <span class="n">before all of these</span> <span class="w">{hidden ? `(${hidden} older edits are not listed)` : 'the cave as the server last saved it whole'}</span>
        </button>
        {#if keep === 0}<div class="open" transition:slide={{ duration: 200 }}><CavePicture {picture} /></div>{/if}
      </div>
    </div>
  {/if}
  {#if preview}<p class="note prev"><strong>That version:</strong> {preview.empty ? 'nothing surveyed' : `${formatLength(preview.length_m, cave.units, 0)} surveyed · ${preview.stations} stations${preview.depth_m === null ? '' : ` · ${formatLength(preview.depth_m, cave.units, 0)} deep`}`}</p>{/if}
  {#if done}<p class="note ok" role="status">{done}</p>{/if}
  <div class="foot">
    <button onclick={onclose} disabled={busy}>{done ? 'Close' : 'Cancel'}</button>
    <button onclick={open} disabled={keep < 0 || busy || !history} title="A separate new cave with that version; this cave is not touched">{busy ? 'Opening…' : 'Open as a copy'}</button>
    <button class="go" onclick={restore} disabled={keep < 0 || busy || !history || !isOpen} title={isOpen ? 'Put this cave back to that version. Its history is kept: the restore is one more step, and Undo takes it back' : 'Open the cave first, then restore it'}>Restore this cave</button>
  </div>
</div>

<style>
  .veil { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); z-index: 60; }
  .hist { position: fixed; z-index: 61; left: 8px; right: 8px; top: 8vh; bottom: 8vh; display: flex; flex-direction: column; gap: 6px; padding: 12px; background: var(--panel); color: var(--text); border: 1px solid var(--line); border-radius: 10px; max-width: 560px; margin: 0 auto; }
  h3 { margin: 0; font-size: 15px; }
  .note { margin: 0; color: var(--muted); font-size: 12px; }
  .prev { color: var(--text); }
  .ok { color: var(--accent); }
  .err { margin: 0; color: var(--danger); font-size: 12px; }
  .list { flex: 1 1 0; min-height: 0; overflow: auto; display: flex; flex-direction: column; gap: 2px; }
  .entry { display: flex; flex-direction: column; border-radius: 6px; background: var(--panel-2); }
  .entry.on { outline: 1px solid var(--accent); }
  .open { padding: 0 8px 8px; }
  .list button { display: flex; gap: 8px; align-items: baseline; text-align: left; padding: 8px; border: 1px solid transparent; border-radius: 6px; background: var(--panel-2); color: var(--text); font: inherit; font-size: 12px; min-height: 40px; touch-action: manipulation; }
  .list button.on { background: var(--accent); border-color: var(--accent); color: var(--on-accent, #fff); border-radius: 6px 6px 0 0; }
  .n { flex: none; color: var(--muted); min-width: 4.5em; }
  .on .n { color: inherit; }
  .w { min-width: 0; flex: 1 1 0; overflow-wrap: anywhere; }
  .m { flex: none; font-size: 11px; color: var(--muted); white-space: nowrap; }
  .on .m { color: inherit; }
  .foot { display: flex; justify-content: flex-end; gap: 8px; }
  .foot button { min-height: 40px; padding: 0 14px; border: 1px solid var(--line); border-radius: 8px; background: var(--panel-2); color: var(--text); font: inherit; }
  .foot .go { background: var(--accent); border-color: var(--accent); color: var(--on-accent, #fff); font-weight: 600; }
  .foot button:disabled { opacity: 0.5; }
</style>
