<script lang="ts">
  // The pattern editor: linetypes (dashes, dots and motifs along a line), fills (hatches, dots and tiled motifs inside an area) and
  // the motifs they repeat. Patterns that come with CaveCAD are copied to change; the cave's own are kept in the cave file.
  import { onMount } from 'svelte';
  import { cave, changed } from '../lib/cave.svelte';
  import { engine, loadEngine } from '../lib/engine';
  import { drawing } from '../lib/line-style.svelte';
  import { patternFocus } from '../lib/pattern-focus.svelte';
  import { untrack } from 'svelte';
  import { drawMarks, drawMotif } from '../lib/patterns';
  import PatternCanvas from '../components/patterns/PatternCanvas.svelte';
  import { copyName, plain, type FillDef, type LineDef, type Motif, type PatternKind, type PatternLibrary } from '../lib/patterns';
  import FillEditor from '../components/patterns/FillEditor.svelte';
  import LineEditor from '../components/patterns/LineEditor.svelte';
  import MotifEditor from '../components/patterns/MotifEditor.svelte';
  import { acceptShare, dropFromLibrary, keepInLibrary, library, shareLink, useInCave, type LibraryDef } from '../lib/pattern-library.svelte';
  import { account } from '../lib/account.svelte';

  let ready = $state(false);
  let kind = $state<PatternKind>('line');
  /** The pattern on show: the cave's (or one that comes with CaveCAD), or one from the account library (`from`). */
  let picked = $state<{ kind: PatternKind; name: string; from?: 'library' } | null>(null);
  let message = $state('');
  onMount(() => {
    loadEngine().then(() => (ready = true));
  });

  const lib = $derived.by((): PatternLibrary => {
    void cave.version; // edits and undo re-read
    return ready ? engine.patternLibrary() : { motifs: [], lines: [], fills: [] };
  });
  const items = $derived(kind === 'line' ? lib.lines : kind === 'fill' ? lib.fills : lib.motifs);
  const current = $derived(picked && picked.kind === kind && picked.from !== 'library' ? items.find((i) => i.name === picked!.name) : undefined);
  /** Library patterns of this kind that are not in the cave already (a cave pattern of the same name shows instead). */
  const libraryOnly = $derived(library.items.filter((i) => i.kind === kind && !items.some((c) => c.own && c.name === i.name)));
  /** The library pattern on show, if one is. */
  const libPicked = $derived(picked?.from === 'library' && picked.kind === kind ? library.items.find((i) => i.kind === kind && i.name === picked!.name) : undefined);
  const signedIn = $derived(account.phase === 'ready' || account.phase === 'offline');
  const mineHere = $derived(items.filter((it) => it.own));
  const builtin = $derived(items.filter((it) => !it.own));
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
  let listEl = $state<HTMLDivElement>();
  // The picked pattern stays in sight in the list (after a save, a copy, or a pick from elsewhere).
  $effect(() => {
    void picked;
    void items;
    queueMicrotask(() => listEl?.querySelector('.item.on')?.scrollIntoView({ block: 'nearest' }));
  });

  let patInput = $state<HTMLInputElement>();
  let realSize = $state(false);
  let patUnits = $state<'mm' | 'in'>('mm');
  /** Reads .pat files into the cave's own fills; a name already taken gets " copy". */
  async function importPat(input: HTMLInputElement) {
    const files = [...(input.files ?? [])];
    input.value = '';
    if (!files.length || !mayLeave()) return;
    let made = 0;
    const said: string[] = [];
    const taken = [...names];
    let last = '';
    for (const f of files) {
      let text = await f.text();
      // a file that says it is in inches is read as inches either way; picking inches covers files that do not say
      if (patUnits === 'in') text = ';%UNITS=INCH\n' + text.replace(/^\s*;\s*%\s*UNITS\s*=\s*INCH.*$/gim, '');
      const got = engine.readPat(text, realSize ? 1 / 16 : 1);
      if (typeof got === 'string') {
        said.push(`${f.name}: ${got}`);
        continue;
      }
      for (const d of got) {
        const name = taken.includes(d.name) ? copyName(d.name, taken) : d.name;
        const r = engine.savePattern('fill', { ...d, name });
        if (r === 'ok') {
          if (signedIn) void keepInLibrary('fill', { ...d, name } as LibraryDef);
          taken.push(name);
          made++;
          last = name;
        } else said.push(`${d.name}: ${r.replace(/^error: /, '')}`);
      }
    }
    if (made) changed();
    pendingNew = null;
    if (last) picked = { kind: 'fill', name: last };
    message = [made ? `Imported ${made} pattern${made === 1 ? '' : 's'} into your fills.` : '', ...said].filter(Boolean).join(' ');
  }

  // Opened from a line or area ("Edit pattern…" in the ribbon or the right-click menu): turn to its pattern.
  let focusSeen = patternFocus.seq;
  $effect(() => {
    const seq = patternFocus.seq;
    if (seq === focusSeen) return;
    focusSeen = seq;
    untrack(() => {
      if (!mayLeave()) return;
      pendingNew = null;
      message = '';
      kind = patternFocus.kind;
      picked = patternFocus.name ? { kind: patternFocus.kind, name: patternFocus.name } : null;
    });
  });

  /** The editor has changes not saved yet. */
  let dirty = $state(false);
  /** True when it is fine to leave what is in the editor (nothing unsaved, or the caver said to throw it away). */
  function mayLeave(): boolean {
    if (!dirty && !(pendingNew && !current)) return true;
    const what = shown?.name ?? 'this pattern';
    if (!confirm(`“${what}” has changes that are not saved. Throw them away?`)) return false;
    dirty = false;
    return true;
  }
  function setKind(k: PatternKind) {
    if (k === kind || !mayLeave()) return;
    pendingNew = null;
    kind = k;
  }

  function create() {
    if (!mayLeave()) return;
    const def = blank();
    pendingNew = { ...def, own: true } as LineDef | FillDef | Motif;
    picked = { kind, name: def.name };
  }
  let pendingNew = $state<(LineDef | FillDef | Motif) | null>(null);
  const shown = $derived(
    current ??
      (picked && picked.kind === kind && pendingNew && pendingNew.name === picked.name && picked.from !== 'library' ? pendingNew : undefined) ??
      (libPicked ? ({ ...(libPicked.def as object), name: libPicked.name, own: false } as LineDef | FillDef | Motif) : undefined),
  );

  function duplicate() {
    if (!shown || !mayLeave()) return;
    const copy = { ...plain(shown), name: copyName(shown.name, names), own: true } as LineDef | FillDef | Motif;
    pendingNew = copy;
    picked = { kind, name: copy.name };
  }
  function pick(name: string, from?: 'library') {
    if (picked?.kind === kind && picked.name === name && picked.from === from) return;
    if (!mayLeave()) return;
    message = '';
    pendingNew = null;
    picked = from ? { kind, name, from } : { kind, name };
  }
  /** Puts the library pattern on show into the cave, then shows the cave's copy. */
  function useFromLibrary() {
    if (!libPicked) return;
    if (!cave.hasCave) {
      message = 'Open a cave first.';
      return;
    }
    const why = useInCave(libPicked);
    if (why) {
      message = why;
      return;
    }
    changed();
    const name = libPicked.name;
    picked = { kind, name };
    message = `“${name}” is in this cave now.`;
  }
  async function share() {
    const name = shown?.name;
    if (!name) return;
    // a cave pattern not in the library yet (saved before sign-in) goes there first, so it can be shared
    if (current?.own && !library.items.some((i) => i.kind === kind && i.name === name)) await keepInLibrary(kind, current as LibraryDef);
    const link = await shareLink([{ kind, name }]);
    if (!link) {
      message = library.problem || 'The share link could not be made.';
      return;
    }
    let copied = false;
    try {
      await navigator.clipboard.writeText(link);
      copied = true;
    } catch {
      /* the link is shown to copy by hand */
    }
    message = `${copied ? 'Link copied' : 'Share link'}: ${link} — anyone signed in to CaveCAD who opens it can add a copy to their library.`;
  }
  async function removeFromLibrary() {
    if (!libPicked || !confirm(`Take “${libPicked.name}” out of your library? Caves that use it keep their own copy.`)) return;
    await dropFromLibrary(kind, libPicked.name);
    picked = null;
    message = library.problem || 'Taken out of your library.';
  }
  async function takeShare() {
    const from = library.incoming?.from ?? '';
    const n = await acceptShare();
    message = n ? `Added ${n} pattern${n === 1 ? '' : 's'} from ${from} to your library. Pick one under My library, then Use in this cave.` : library.problem || 'Nothing was added.';
  }
  function saved(name: string) {
    // every pattern saved in a cave is kept in the account library too
    const now = engine.patternLibrary();
    const def = (kind === 'line' ? now.lines : kind === 'fill' ? now.fills : now.motifs).find((d) => d.name === name);
    if (def && signedIn) void keepInLibrary(kind, def as LibraryDef);
    pendingNew = null;
    picked = { kind, name };
    dirty = false;
    message =
      kind === 'line'
        ? `Saved “${name}”. To use it, select lines on the map and press Use on selected lines below, or pick it under Line style in the ribbon.`
        : `Saved “${name}”.`;
  }
  function removed() {
    dirty = false;
    message = '';
    picked = null;
    pendingNew = null;
  }
  /** A short straight sample of a linetype for the list. */
  function swatch(def: LineDef) {
    return (g: CanvasRenderingContext2D, w: number, h: number) => {
      const scale = drawing.scale;
      const pxPerM = (3.78 * 1.5 * 1000) / scale;
      const len = (w - 8) / pxPerM;
      try {
        const marks = engine.linePatternMarks(plain(def), [[0, 0], [len, 0]], scale);
        drawMarks(g, marks, ([x, y]) => [4 + x * pxPerM, h / 2 - y * pxPerM], pxPerM);
      } catch {
        /* a pattern that cannot be laid out shows no sample */
      }
    };
  }

  /** A small patch of a fill for the list. */
  function fillSwatch(def: FillDef) {
    return (g: CanvasRenderingContext2D, w: number, h: number) => {
      const scale = drawing.scale;
      // zoomed so a few repeats show: the pattern's typical spacing comes out about 7 pixels
      const sizes = def.layers
        .map((l) => (l.t === 'tile' ? l.across_mm : l.t === 'patlines' ? Math.abs(l.spacing_mm) : l.spacing_mm))
        .filter((v) => v > 0)
        .sort((a, b) => a - b);
      const typical = sizes.length ? sizes[Math.floor(sizes.length / 2)] : 3;
      const pxPerMm = Math.max(0.3, Math.min(3.78, 7 / typical));
      const pxPerM = (pxPerMm * 1000) / scale;
      const hx = w / 2 / pxPerM;
      const hy = h / 2 / pxPerM;
      try {
        const marks = engine.fillPatternMarks(plain(def), [[-hx, -hy], [hx, -hy], [hx, hy], [-hx, hy]], 'swatch', 1, scale);
        g.save();
        if (marks.tint) {
          g.globalAlpha = 0.25;
          g.fillRect(0, 0, w, h);
          g.globalAlpha = 1;
        }
        drawMarks(g, marks, ([x, y]) => [w / 2 + x * pxPerM, h / 2 - y * pxPerM], pxPerM);
        g.restore();
      } catch {
        /* a fill that cannot be laid out shows no sample */
      }
    };
  }
  /** A motif's drawing, a few times along, for the list. */
  function motifSwatch(def: Motif) {
    return (g: CanvasRenderingContext2D, w: number, h: number) => {
      for (let x = h / 2 + 2; x < w - h / 2; x += h * 1.4) drawMotif(g, def.prims, x, h / 2, h * 0.7);
    };
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
      <button role="tab" aria-selected={kind === k} class:on={kind === k} onclick={() => setKind(k as PatternKind)}>{TITLES[k as PatternKind]}</button>
    {/each}
  </div>
  <p class="help">{HELP[kind]}</p>
  {#if !cave.hasCave}
    <p class="help">Open a cave to make patterns of your own. The ones that come with CaveCAD can be looked at.</p>
  {/if}
  <div class="body">
    <div class="list" bind:this={listEl}>
      <button class="new" onclick={create} disabled={!cave.hasCave}>+ New</button>
      {#if kind === 'fill'}
        <button class="new" onclick={() => patInput?.click()} disabled={!cave.hasCave} title="Read an AutoCAD hatch pattern file (.pat) into your fills">Import .pat…</button>
        <label class="small-check col" title="acadiso.pat and most metric files are millimetres; AutoCAD's acad.pat is inches">File units
          <select bind:value={patUnits} aria-label="Units of the .pat file">
            <option value="mm">Millimetres (acadiso.pat)</option>
            <option value="in">Inches (acad.pat)</option>
          </select>
        </label>
        <label class="small-check" title="Architectural patterns (bricks, boards, roofing) are drawn at their real size; this draws them at 1/16 so they suit a map"><input type="checkbox" bind:checked={realSize} /> Real-size patterns (shrink 1/16)</label>
        <input bind:this={patInput} type="file" accept=".pat,text/plain" multiple hidden onchange={(e) => importPat(e.currentTarget)} />
      {/if}
      {#snippet entry(it: LineDef | FillDef | Motif, from: 'library' | undefined, title: string)}
        <button class="item" class:on={picked?.kind === kind && picked.name === it.name && picked.from === from} onclick={() => pick(it.name, from)} {title}>
          <span class="label">{it.name}</span>
          {#if kind === 'line'}<span class="swatch"><PatternCanvas draw={swatch(it as LineDef)} height={18} /></span>
          {:else if kind === 'fill'}<span class="swatch"><PatternCanvas draw={fillSwatch(it as FillDef)} height={26} /></span>
          {:else}<span class="swatch icon"><PatternCanvas draw={motifSwatch(it as Motif)} height={26} /></span>{/if}
        </button>
      {/snippet}
      {#if mineHere.length}
        <div class="group">In this cave</div>
        {#each mineHere as it (it.name)}{@render entry(it, undefined, it.name)}{/each}
      {/if}
      <div class="group">My library</div>
      {#if !signedIn}
        <p class="note">Sign in to keep your patterns in your account, for every cave.</p>
      {:else if !libraryOnly.length}
        <p class="note">{library.loaded ? 'Patterns you save are kept here too, for every cave.' : 'Reading your library…'}</p>
      {:else}
        {#each libraryOnly as li (li.name)}{@render entry({ ...(li.def as object), name: li.name } as LineDef | FillDef | Motif, 'library', `${li.name}: in your library`)}{/each}
      {/if}
      {#if builtin.length}
        <div class="group">Comes with CaveCAD</div>
        {#each builtin as it (it.name)}{@render entry(it, undefined, `${it.name}: comes with CaveCAD, copy it to change it`)}{/each}
      {/if}
      {#if pendingNew && picked && picked.kind === kind && !current && picked.from !== 'library'}
        <button class="item on">{pendingNew.name}<small>not saved</small></button>
      {/if}
    </div>
    <div class="edit">
      {#if library.incoming}
        <div class="incoming" role="status">
          <span><b>{library.incoming.from}</b> shared {library.incoming.items.length} pattern{library.incoming.items.length === 1 ? '' : 's'} with you: {library.incoming.items.map((i) => i.name).join(', ')}.</span>
          <button class="primary" onclick={takeShare}>Add to my library</button>
          <button onclick={() => (library.incoming = null)}>Not now</button>
        </div>
      {/if}
      {#if message}<p class="saved" role="status">{message}</p>{/if}
      {#if shown && signedIn && (current?.own || libPicked)}
        <div class="libbar">
          {#if libPicked}<span class="muted">In your library{items.some((c) => c.own && c.name === libPicked.name) ? '' : ', not in this cave yet'}.</span>{:else}<span class="muted">In this cave, and kept in your library.</span>{/if}
          <button onclick={share} title="Make a link that lets other CaveCAD users add a copy of this pattern to their library">Share…</button>
          {#if libPicked}<button class="danger" onclick={removeFromLibrary}>Remove from my library…</button>{/if}
        </div>
      {/if}
      {#if !shown}
        <p class="help">Pick one to look at it, or press New.</p>
      {:else}
        {#key `${kind}:${shown.name}:${current ? 'saved' : libPicked ? 'library' : 'new'}`}
          {#if kind === 'line'}
            <LineEditor def={shown as LineDef} {lib} onsaved={saved} onremoved={removed} onduplicate={libPicked ? useFromLibrary : duplicate} ondirty={(d) => (dirty = d)} unsaved={!current && !libPicked} fromLibrary={!!libPicked} />
          {:else if kind === 'fill'}
            <FillEditor def={shown as FillDef} {lib} onsaved={saved} onremoved={removed} onduplicate={libPicked ? useFromLibrary : duplicate} ondirty={(d) => (dirty = d)} unsaved={!current && !libPicked} fromLibrary={!!libPicked} />
          {:else}
            <MotifEditor def={shown as Motif} onsaved={saved} onremoved={removed} onduplicate={libPicked ? useFromLibrary : duplicate} ondirty={(d) => (dirty = d)} unsaved={!current && !libPicked} fromLibrary={!!libPicked} />
          {/if}
        {/key}
      {/if}
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
  .list { width: 170px; flex: none; display: flex; flex-direction: column; gap: 2px; overflow: auto; }
  .saved { margin: 0 0 8px; padding: 6px 8px; font-size: 12px; border-radius: var(--radius); background: var(--accent-bg); }
  .small-check.col { flex-direction: column; gap: 2px; }
  .small-check select { font-size: 11px; width: 100%; }
  .small-check { display: flex; gap: 4px; align-items: flex-start; font-size: 10px; color: var(--muted); margin: 0 2px 4px; }
  .note { margin: 0 2px 4px; font-size: 11px; color: var(--muted); }
  .incoming { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; margin: 0 0 8px; padding: 8px; border: 1px solid var(--accent); border-radius: var(--radius); background: var(--accent-bg); font-size: 12px; }
  .incoming span { flex: 1; min-width: 180px; }
  .libbar { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; margin: 0 0 8px; font-size: 12px; }
  .libbar .muted { color: var(--muted); flex: 1; }
  .incoming button, .libbar button { font: inherit; padding: 3px 8px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); cursor: pointer; }
  .incoming .primary { background: var(--accent-bg); border-color: var(--accent); }
  .libbar .danger { color: var(--danger, #c0392b); }
  .group { margin: 6px 2px 2px; font-size: 10px; color: var(--muted); text-transform: uppercase; letter-spacing: 0.05em; }
  .label { display: block; }
  .swatch { display: block; margin-top: 3px; }
  .swatch :global(canvas) { border: none; background: none; }
  .item, .new { text-align: left; padding: 5px 8px; font: inherit; color: var(--text); background: none; border: 1px solid transparent; border-radius: var(--radius); cursor: pointer; }
  .new { border-color: var(--line); background: var(--panel-2); margin-bottom: 4px; }
  .new:disabled { opacity: 0.5; cursor: default; }
  .item:hover { background: var(--panel-2); }
  .item.on { background: var(--accent-bg); border-color: var(--accent); }
  small { display: block; font-size: 9px; color: var(--muted); letter-spacing: 0.05em; text-transform: uppercase; }
  .edit { flex: 1; min-width: 0; overflow: auto; padding-right: 4px; }
</style>
