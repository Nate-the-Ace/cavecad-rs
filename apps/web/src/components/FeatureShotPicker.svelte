<script lang="ts">
  // Feature shot: pick what the next shot points at (a common symbol, or any from the full list) and an optional label.
  // The shot then places that symbol where it hits; it is not written to the notebook, so it is no leg, wall or splay.
  import { X } from '@lucide/svelte';
  import { engine, loadEngine } from '../lib/engine';
  import { drawSymbol } from '../lib/symbols';

  let { onpick, onclose }: { onpick: (symbol: string, label: string) => void; onclose: () => void } = $props();

  /** What cavers mark most often, in the order they are looked for. */
  const COMMON = [
    'sym-stalactite', 'sym-stalagmite', 'sym-column', 'sym-flowstone', 'sym-soda-straw', 'sym-helictite',
    'sym-bat', 'sym-guano', 'sym-drip-seep', 'sym-spring', 'sym-sump', 'sym-flow-arrow',
    'sym-pit', 'sym-dome', 'sym-breakdown', 'sym-continuation', 'sym-air-draught', 'sym-dig',
    'sym-bones', 'sym-archeo-material', 'sym-bolt', 'sym-natural-anchor', 'sym-danger', 'sym-camp',
  ];

  let all = $state<{ key: string; name: string; category: string }[]>([]);
  let label = $state('');
  let more = $state('');
  loadEngine().then(() => (all = engine.symbols().map((s) => ({ key: s.key, name: s.name, category: s.category }))));
  const common = $derived(COMMON.map((k) => all.find((s) => s.key === k)).filter((s) => !!s));
  const categories = $derived([...new Set(all.map((s) => s.category))]);

  /** Draws a symbol into a small canvas, in the text colour. */
  function glyph(c: HTMLCanvasElement, key: string) {
    const g = c.getContext('2d');
    if (!g) return;
    const dpr = window.devicePixelRatio || 1;
    c.width = 44 * dpr;
    c.height = 44 * dpr;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    const ink = getComputedStyle(c).color;
    g.strokeStyle = ink;
    g.fillStyle = ink;
    g.lineWidth = 1.4;
    try {
      drawSymbol(g, key, [22, 22], [36, 22]);
    } catch {
      // a symbol that cannot be drawn small still shows its name
    }
  }
  const pick = (key: string) => onpick(key, label.trim());
</script>

<div class="veil" role="presentation" onclick={onclose}>
  <div class="box" role="dialog" aria-modal="true" aria-label="Feature shot" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === 'Escape' && onclose()}>
    <header>
      <b>Feature shot</b>
      <button onclick={onclose} aria-label="Close"><X size={20} /></button>
    </header>
    <p class="hint">Pick what you are pointing at, then shoot it (the button, or DIST on the X1). The symbol goes where the laser hits. It is not written to the notebook: no leg, wall or splay.</p>
    <label class="lab">Label <input type="text" bind:value={label} placeholder="Optional: Bat roost, 3 m drop…" maxlength="80" /></label>
    <div class="grid">
      {#each common as s (s.key)}
        <button class="tile" onclick={() => pick(s.key)} title={s.name}>
          <canvas use:glyph={s.key} aria-hidden="true"></canvas>
          <span>{s.name}</span>
        </button>
      {/each}
    </div>
    <label class="lab">More
      <select bind:value={more} onchange={() => more && pick(more)}>
        <option value="">Any symbol…</option>
        {#each categories as cat (cat)}
          <optgroup label={cat}>{#each all.filter((s) => s.category === cat) as s (s.key)}<option value={s.key}>{s.name}</option>{/each}</optgroup>
        {/each}
      </select>
    </label>
  </div>
</div>

<style>
  .veil { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); display: grid; place-items: center; z-index: 75; }
  .box { width: min(560px, 94vw); max-height: 90vh; overflow: auto; background: var(--panel); color: var(--text); border: 1px solid var(--line); border-radius: 10px; padding: 0 14px 14px; outline: none; }
  header { display: flex; align-items: center; justify-content: space-between; padding: 10px 0 4px; }
  header button { min-height: 40px; min-width: 40px; display: grid; place-items: center; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); }
  .hint { margin: 0 0 8px; font-size: 12px; color: var(--muted); }
  .lab { display: flex; align-items: center; gap: 8px; font-size: 13px; margin: 8px 0; }
  .lab input, .lab select { flex: 1; min-width: 0; min-height: 38px; padding: 0 8px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); font: inherit; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(84px, 1fr)); gap: 6px; }
  .tile { display: flex; flex-direction: column; align-items: center; gap: 2px; padding: 6px 4px; min-height: 76px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); cursor: pointer; font: inherit; font-size: 11px; line-height: 1.2; text-align: center; }
  .tile:hover { border-color: var(--accent); }
  canvas { width: 44px; height: 44px; }
  @media (max-width: 600px) {
    .box { width: 100vw; height: 100dvh; max-height: none; border-radius: 0; border: 0; padding-top: env(safe-area-inset-top); padding-bottom: env(safe-area-inset-bottom); box-sizing: border-box; }
  }
</style>
