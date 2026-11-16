<script lang="ts">
  // A sweep's coverage as a dial of 12 sectors: filled where shots landed, outlined where nothing has yet.
  // LRUD ring: looking along the passage (ceiling at the top, left wall on the left). Room sweep: north at the top.
  import type { SweepCheck } from '../lib/survey-types';

  let { check, size = 132 }: { check: SweepCheck; size?: number } = $props();

  const c = 60;
  const outer = 54;
  const inner = 30;
  const n = $derived(check.sectors.length || 12);
  const most = $derived(Math.max(1, ...check.sectors));

  /** Sector k as math angles (degrees, anticlockwise from the right), whichever way the kind counts. */
  function span(k: number): [number, number] {
    const w = 360 / n;
    if (check.kind === 'room') return [90 - (k + 1) * w, 90 - k * w]; // bearings clockwise from north
    return [k * w, (k + 1) * w];
  }

  const pt = (a: number, r: number) => {
    const t = (a * Math.PI) / 180;
    return `${(c + r * Math.cos(t)).toFixed(2)} ${(c - r * Math.sin(t)).toFixed(2)}`;
  };

  function wedge(k: number): string {
    const [a0, a1] = span(k);
    const g = 1.2; // a hair between sectors
    return `M ${pt(a0 + g, inner)} L ${pt(a0 + g, outer)} A ${outer} ${outer} 0 0 0 ${pt(a1 - g, outer)} L ${pt(a1 - g, inner)} A ${inner} ${inner} 0 0 1 ${pt(a0 + g, inner)} Z`;
  }

  const labels = $derived(
    check.kind === 'room'
      ? [
          { t: 'N', x: c, y: 8 },
          { t: 'E', x: 113, y: c + 4 },
          { t: 'S', x: c, y: 117 },
          { t: 'W', x: 7, y: c + 4 },
        ]
      : [
          { t: 'Ceiling', x: c, y: 8 },
          { t: 'R', x: 114, y: c + 4 },
          { t: 'Floor', x: c, y: 117 },
          { t: 'L', x: 6, y: c + 4 },
        ],
  );
</script>

<svg viewBox="-4 -4 128 128" width={size} height={size} role="img" aria-label="Sweep coverage: {check.sectors.filter((s) => s > 0).length} of {n} directions reached">
  {#each check.sectors as count, k (k)}
    <path d={wedge(k)} class:hit={count > 0} style:opacity={count > 0 ? 0.45 + 0.55 * (count / most) : 1} />
  {/each}
  <text x={c} y={c - 2} class="num">{check.usable}</text>
  <text x={c} y={c + 12} class="of">of {check.wanted}</text>
  {#each labels as l (l.t)}
    <text x={l.x} y={l.y} class="lab">{l.t}</text>
  {/each}
</svg>

<style>
  svg { display: block; flex: none; }
  path { fill: none; stroke: var(--warn); stroke-width: 1.2; stroke-dasharray: 3 2; }
  path.hit { fill: var(--accent); stroke: var(--accent); stroke-dasharray: none; }
  text { text-anchor: middle; fill: var(--text); font-family: inherit; }
  .num { font-size: 20px; font-weight: 700; font-variant-numeric: tabular-nums; }
  .of { font-size: 9px; fill: var(--muted); }
  .lab { font-size: 9px; fill: var(--muted); }
</style>
