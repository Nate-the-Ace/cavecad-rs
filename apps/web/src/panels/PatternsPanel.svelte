<script lang="ts">
  // The pattern editor: linetypes (dashes, dots and motifs along a line), fills (hatches, dots and tiled motifs inside an area) and
  // the motifs they repeat. Patterns that come with CaveCAD are copied to change; the cave's own are kept in the cave file.
  import { onMount } from 'svelte';
  import { cave, changed } from '../lib/cave.svelte';
  import { engine, loadEngine } from '../lib/engine';
  import { copyName, plain, type FillDef, type LineDef, type Motif, type PatternKind, type PatternLibrary } from '../lib/patterns';
  import FillEditor from '../components/patterns/FillEditor.svelte';
  import LineEditor from '../components/patterns/LineEditor.svelte';
  import MotifEditor from '../components/patterns/MotifEditor.svelte';

  let ready = $state(false);
  let kind = $state<PatternKind>('line');
  let picked = $state<{ kind: PatternKind; name: string } | null>(null);
  let message = $state('');
  onMount(() => {
    loadEngine().then(() => (ready = true));
  });

  const lib = $derived.by((): PatternLibrary => {
    void cave.version; // edits and undo re-read
    return ready ? engine.patternLibrary() : { motifs: [], lines: [], fills: [] };
  });
  const items = $derived(kind === 'line' ? lib.lines : kind === 'fill' ? lib.fills : lib.motifs);
  const current = $derived(picked && picked.kind === kind ? items.find((i) => i.name === picked!.name) : undefined);
  const names = $derived(items.map((i) => i.name));

  const TITLES: Record<PatternKind, string> = { line: 'Linetypes', fill: 'Fills', motif: 'Motifs' };
  const HELP: Record<PatternKind, string> = {
    line: 'A repeating pattern for a line: dashes, gaps, dots and motifs.',
    fill: 'A repeating pattern inside an area: hatch lines, dots and tiled motifs.',
    motif: 'A small drawing that repeats: a tick, a chevron, a little rock.',
  };

  function blank(): LineDef | FillDef | Motif {
    const name = copyName(kind === 'line' ? 'New linetype' : kind === 'fill' ? 'New fill' : 'New motif', names).replace(' copy', '');
    const firstMotif = lib.motifs[0]?.name ?? '';
    if (kind === 'line') return { name, elements: [{ t: 'dash', mm: 3 }, { t: 'gap', mm: 2 }], own: false };
    if (kind === 'fill') return { name, layers: [{ t: 'hatch', angle_deg: 45, spacing_mm: 3 }], tint: false, own: false, ...(firstMotif ? {} : {}) };
    return { name, prims: [], own: false };
  }

  /** Starts a new pattern: shown in its editor as the cave's own, saved once the caver presses Save. */
  function create() {
    const def = blank();
    pendingNew = { ...def, own: true } as LineDef | FillDef | Motif;
    picked = { kind, name: def.name };
  }
  let pendingNew = $state<(LineDef | FillDef | Motif) | null>(null);
  const shown = $derived(current ?? (picked && picked.kind === kind && pendingNew && pendingNew.name === picked.name ? pendingNew : undefined));

  function duplicate() {
    if (!shown) return;
    const copy = { ...plain(shown), name: copyName(shown.name, names), own: true } as LineDef | FillDef | Motif;
    pendingNew = copy;
    picked = { kind, name: copy.name };
  }
  function pick(name: string) {
    pendingNew = null;
    picked = { kind, name };
  }
  function saved(name: string) {
    pendingNew = null;
    picked = { kind, name };
    message = '';
  }
  function removed() {
    picked = null;
    pendingNew = null;
  }
  function undo() {
    // the cave's own undo covers saves and removals; this is a pointer to it, not a second history
    changed();
  }
  void undo;
</script>

<div class="panel">
  <div class="tabs" role="tablist">
    {#each ['line', 'fill', 'motif'] as k}
      <button role="tab" aria-selected={kind === k} class:on={kind === k} onclick={() => (kind = k as PatternKind)}>{TITLES[k as PatternKind]}</button>
    {/each}
  </div>
  <p class="help">{HELP[kind]}</p>
  {#if !cave.hasCave}
    <p class="help">Open a cave to make patterns of your own. The ones that come with CaveCAD can be looked at.</p>
  {/if}
  <div class="body">
    <div class="list">
      <button class="new" onclick={create} disabled={!cave.hasCave}>+ New</button>
      {#each items as it (it.name)}
        <button class="item" class:on={picked?.kind === kind && picked.name === it.name} onclick={() => pick(it.name)}>{it.name}{#if !it.own}<small>built in</small>{/if}</button>
      {/each}
      {#if pendingNew && picked && picked.kind === kind && !current}
        <button class="item on">{pendingNew.name}<small>not saved</small></button>
      {/if}
    </div>
    <div class="edit">
      {#if !shown}
        <p class="help">Pick one to look at it, or press New.</p>
      {:else}
        {#key `${kind}:${shown.name}:${current ? 'saved' : 'new'}`}
          {#if kind === 'line'}
            <LineEditor def={shown as LineDef} {lib} onsaved={saved} onremoved={removed} onduplicate={duplicate} />
          {:else if kind === 'fill'}
            <FillEditor def={shown as FillDef} {lib} onsaved={saved} onremoved={removed} onduplicate={duplicate} />
          {:else}
            <MotifEditor def={shown as Motif} onsaved={saved} onremoved={removed} onduplicate={duplicate} />
          {/if}
        {/key}
      {/if}
      {#if message}<p class="help">{message}</p>{/if}
    </div>
  </div>
</div>

<style>
  .panel { display: flex; flex-direction: column; height: 100%; background: var(--panel); color: var(--text); overflow: hidden; }
  .tabs { display: flex; border-bottom: 1px solid var(--line); }
  .tabs button { flex: 1; padding: 8px; background: none; border: none; border-bottom: 2px solid transparent; color: var(--muted); font: inherit; cursor: pointer; }
  .tabs button.on { color: var(--text); border-bottom-color: var(--accent); }
  .help { margin: 6px 10px 0; font-size: 12px; color: var(--muted); }
  .body { display: flex; flex: 1; min-height: 0; gap: 10px; padding: 8px 10px 10px; }
  .list { width: 150px; flex: none; display: flex; flex-direction: column; gap: 2px; overflow: auto; }
  .item, .new { text-align: left; padding: 5px 8px; font: inherit; color: var(--text); background: none; border: 1px solid transparent; border-radius: var(--radius); cursor: pointer; }
  .new { border-color: var(--line); background: var(--panel-2); margin-bottom: 4px; }
  .new:disabled { opacity: 0.5; cursor: default; }
  .item:hover { background: var(--panel-2); }
  .item.on { background: var(--accent-bg); border-color: var(--accent); }
  small { display: block; font-size: 9px; color: var(--muted); letter-spacing: 0.05em; text-transform: uppercase; }
  .edit { flex: 1; min-width: 0; overflow: auto; padding-right: 4px; }
</style>
