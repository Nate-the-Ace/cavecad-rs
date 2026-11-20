<script lang="ts">
  // A small picture of a gimbal scan's detail: every planned shot as a dot (faded where the gimbal cannot reach).
  // Ring: the cross-section seen from the station looking along the passage (left wall left, up up). Dome: the
  // half-sphere seen from straight above, front up (straight up in the middle, the horizon at the rim).
  import type { Direction } from '../lib/instrument/gimbal-ik';
  // while a scan runs: `taken` holds the keys of shots done, `current` the one being taken
  let { kind, points, taken = [], current = '' }: { kind: 'ring' | 'dome'; points: (Direction & { ok: boolean; key?: string })[]; taken?: string[]; current?: string } = $props();
  const done = $derived(new Set(taken));
  const running = $derived(taken.length > 0 || !!current);
  const R = 40;
  const rad = (d: number) => (d * Math.PI) / 180;
  const dotR = $derived(Math.max(0.55, Math.min(2.6, 70 / Math.max(points.length, 1) ** (kind === 'dome' ? 0.5 : 1) / 2)));
  // ring: azimuth +90 is the right wall; inclination up from level
  const ringXY = (d: Direction) => {
    const side = Math.sin(rad(d.az)) >= 0 ? 1 : -1;
    return [side * Math.cos(rad(d.inc)) * R, -Math.sin(rad(d.inc)) * R];
  };
  // dome: distance from the middle grows with how far below straight up; front (az 0) at the top
  const domeXY = (d: Direction) => {
    const r = ((90 - d.inc) / 90) * R;
    return [Math.sin(rad(d.az)) * r, -Math.cos(rad(d.az)) * r];
  };
</script>

{#if kind === 'ring'}
  <svg viewBox="-46 -46 92 70" class="preview" role="img" aria-label="Ring scan: {points.filter((p) => p.ok).length} shots across the passage">
    <path d="M -44 0 A 44 44 0 0 1 44 0" class="guide" />
    <line x1="-46" y1="0" x2="46" y2="0" class="guide" />
    <path d="M -40 0 A 40 40 0 0 0 40 0" class="guide faint" />
    {#each points as p}
      {@const [x, y] = ringXY(p)}
      <line x1="0" y1="0" x2={x} y2={y} class="ray" class:off={!p.ok} class:todo={running && !done.has(p.key ?? '')} />
      <circle cx={x} cy={y} r={p.key === current ? dotR * 1.8 : dotR} class="dot" class:off={!p.ok} class:todo={running && !done.has(p.key ?? '')} class:now={p.key === current} />
    {/each}
    <circle cx="0" cy="0" r="2.2" class="me" />
  </svg>
{:else}
  <svg viewBox="-46 -46 92 92" class="preview" role="img" aria-label="Dome scan: {points.filter((p) => p.ok).length} shots over the half-sphere">
    <circle cx="0" cy="0" r="44" class="guide" />
    <circle cx="0" cy="0" r="22" class="guide faint" />
    <path d="M 0 -46 l -3 5 h 6 z" class="me" />
    {#each points as p}
      {@const [x, y] = domeXY(p)}
      <circle cx={x} cy={y} r={p.key === current ? dotR * 1.8 : dotR} class="dot" class:off={!p.ok} class:todo={running && !done.has(p.key ?? '')} class:now={p.key === current} />
    {/each}
  </svg>
{/if}

<style>
  .preview { width: 92px; height: auto; flex: none; color: var(--accent); }
  .guide { fill: none; stroke: var(--border, #8886); stroke-width: 1; }
  .guide.faint { stroke-dasharray: 2 3; }
  .ray { stroke: currentColor; stroke-width: 0.5; opacity: 0.35; }
  .ray.off { opacity: 0.08; }
  .dot { fill: currentColor; }
  .dot.off { fill: var(--text, #888); opacity: 0.25; }
  /* while running: still to do is hollow, done is solid, the one being taken pulses */
  .dot.todo:not(.off):not(.now) { fill: none; stroke: currentColor; stroke-width: 0.6; opacity: 0.6; }
  .ray.todo { opacity: 0.1; }
  .dot.now { fill: #e8a33d; stroke: none; opacity: 1; animation: pulse 0.8s ease-in-out infinite alternate; }
  @keyframes pulse { from { opacity: 1; } to { opacity: 0.45; } }
  @media (prefers-reduced-motion: reduce) { .dot.now { animation: none; } }
  .me { fill: var(--text, #888); }
</style>
