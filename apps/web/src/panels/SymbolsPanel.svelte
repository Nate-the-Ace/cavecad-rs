<script lang="ts">
  // The symbol palette: every point symbol as a picture of itself, filed by category. Click one to
  // arm it, then click (or click and drag, to aim and size it) in the Map, Profile or Cross Section.
  import { onMount } from 'svelte';
  import { cave, setTool } from '../lib/cave.svelte';
  import { engine, loadEngine } from '../lib/engine';
  import { drawSymbol, type SymbolDef } from '../lib/symbols';
  import { onTheme } from '../lib/theme';

  let all = $state<SymbolDef[]>([]);
  let query = $state('');
  onMount(() => {
    loadEngine().then(() => (all = engine.symbols()));
  });

  // Two symbol standards are carried: NSS (the US one) and UIS (the international one). Many symbols are in
  // both, so only the chosen standard's version of a shared symbol is shown; one found only in the other
  // standard still is. A search looks through both.
  const STANDARD_KEY = 'cavecad.symbolStandard';
  let standard = $state<'nss' | 'uis'>(readStandard());
  function readStandard(): 'nss' | 'uis' {
    try {
      return localStorage.getItem(STANDARD_KEY) === 'uis' ? 'uis' : 'nss';
    } catch {
      return 'nss';
    }
  }
  function setStandard(s: 'nss' | 'uis') {
    standard = s;
    try {
      localStorage.setItem(STANDARD_KEY, s);
    } catch {
      /* remembered for this visit only */
    }
  }
  /** A symbol's name without its qualifier, so "Pit (depth mark)" and "Pit" are the same symbol. */
  const base = (s: SymbolDef) => s.name.replace(/ \(.*\)$/, '').toLowerCase();
  const shown = $derived.by(() => {
    const q = query.trim().toLowerCase();
    if (q) return all.filter((s) => [s.name, s.uis, s.category].some((t) => t.toLowerCase().includes(q)));
    const preferred = new Set(all.filter((s) => s.set === standard).map(base));
    return all.filter((s) => s.set === standard || (s.set !== 'nss' && s.set !== 'uis') || !preferred.has(base(s)));
  });
  const groups = $derived.by(() => {
    const out: { name: string; items: SymbolDef[] }[] = [];
    for (const s of shown) {
      let g = out.find((x) => x.name === s.category);
      if (!g) out.push((g = { name: s.category, items: [] }));
      g.items.push(s);
    }
    return out;
  });

  /** Draws a symbol's picture into its tile. */
  function thumb(canvas: HTMLCanvasElement, def: SymbolDef) {
    const paint = () => {
      const size = 44;
      const dpr = window.devicePixelRatio || 1;
      canvas.width = size * dpr;
      canvas.height = size * dpr;
      const g = canvas.getContext('2d')!;
      g.scale(dpr, dpr);
      g.clearRect(0, 0, size, size);
      g.strokeStyle = getComputedStyle(canvas).color;
      g.lineWidth = 1.4;
      g.lineCap = 'round';
      g.lineJoin = 'round';
      drawSymbol(g, def.key, [size / 2, size / 2], [size / 2, size / 2 - 18]);
    };
    paint();
    const off = onTheme(paint); // the ink follows the theme
    return { destroy: off };
  }
</script>

<div class="panel">
  <div class="bar">
    <input type="search" placeholder="Search symbols" bind:value={query} aria-label="Search symbols" />
    <div class="standard" role="radiogroup" aria-label="Symbol standard">
      <button role="radio" aria-checked={standard === 'nss'} class:on={standard === 'nss'} title="The NSS symbols used on most US cave maps" onclick={() => setStandard('nss')}>US (NSS)</button>
      <button role="radio" aria-checked={standard === 'uis'} class:on={standard === 'uis'} title="The UIS symbols used on cave maps in much of the rest of the world" onclick={() => setStandard('uis')}>International (UIS)</button>
    </div>
    <span class="hint">{cave.tool.startsWith('sym-') ? 'Click to place, drag to aim and size. Esc to stop.' : 'Pick a symbol, then click in a view.'}</span>
  </div>
  <div class="scroll">
    {#each groups as g}
      <h3>{g.name}</h3>
      <div class="grid">
        {#each g.items as s (s.key)}
          <button class="tile" class:armed={cave.tool === s.key} aria-pressed={cave.tool === s.key} title="{s.name}{s.uis !== s.name ? ` (UIS: ${s.uis})` : ''}{s.set !== 'nss' ? ` · ${s.set.toUpperCase()} set` : ''}" onclick={() => setTool(s.key)}>
            <canvas use:thumb={s} aria-hidden="true"></canvas>
            <span>{s.name}{#if s.set !== standard} <small>{s.set.toUpperCase()}</small>{/if}</span>
          </button>
        {/each}
      </div>
    {:else}
      <p class="none">No symbol matches “{query}”.</p>
    {/each}
  </div>
</div>

<style>
  .panel { display: flex; flex-direction: column; height: 100%; background: var(--panel); color: var(--text); }
  .bar { padding: 8px 10px; display: flex; flex-direction: column; gap: 4px; border-bottom: 1px solid var(--line); }
  input { font: inherit; padding: 5px 8px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); }
  .hint { font-size: 11px; color: var(--muted); }
  .standard { display: flex; gap: 4px; }
  .standard button { flex: 1; font: inherit; font-size: 11px; padding: 3px 6px; border: 1px solid var(--line); border-radius: var(--radius); background: none; color: var(--text); cursor: pointer; }
  .standard button.on { background: var(--accent-bg); border-color: var(--accent); }
  .scroll { overflow: auto; padding: 4px 10px 12px; flex: 1; }
  h3 { margin: 12px 0 6px; font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.06em; color: var(--muted); }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(78px, 1fr)); gap: 6px; }
  .tile { display: flex; flex-direction: column; align-items: center; gap: 3px; padding: 6px 4px; background: none; border: 1px solid transparent; border-radius: var(--radius); cursor: pointer; color: var(--text); font-size: 11px; text-align: center; line-height: 1.15; }
  .tile:hover { background: var(--panel-2); }
  .tile.armed { background: var(--accent-bg); border-color: var(--accent); }
  .tile canvas { width: 44px; height: 44px; color: var(--text); }
  small { display: block; font-size: 9px; color: var(--muted); letter-spacing: 0.05em; }
  .none { color: var(--muted); font-size: 13px; }
</style>
