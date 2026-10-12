<script lang="ts">
  // Cross Section: a cut across the passage anywhere along a surveyed leg, drawn from the walls
  // measured at the two stations (the same rings the 3D tube is built from). Between stations it is
  // a blend and says how far it is from the nearer station.
  import { onMount, untrack } from 'svelte';
  import { cave, filter, wireFilter } from '../lib/cave.svelte';
  import { engine, loadEngine, type Section, type SectionLeg } from '../lib/engine';
  import { formatLength, metresPer, niceNumber, unitName } from '../lib/units';
  import LengthField from '../components/LengthField.svelte';

  let wrap: HTMLDivElement;
  let canvas: HTMLCanvasElement;
  let width = $state(0);
  let height = $state(0);
  let ready = $state(false);
  let legs = $state.raw<SectionLeg[]>([]);
  let shot = $state('');
  let t = $state(1);
  let dist = $state<number | null>(null);
  let section = $state.raw<Section | null>(null);
  let frame = 0;
  let barH = $state(50);

  const leg = $derived(legs.find((l) => l.shot === shot) ?? null);

  // The list of legs follows the cave, the adjustment and the trip filter.
  $effect(() => {
    void cave.version;
    void cave.adjusted;
    void filter.hiddenTrips.length;
    if (!ready) return;
    untrack(() => {
      legs = engine.sectionLegs(cave.adjusted, wireFilter());
      if (!legs.some((l) => l.shot === shot)) {
        shot = legs[0]?.shot ?? '';
        t = 1; // the end station of a leg always has the walls measured with the shot into it
      }
    });
  });

  // The cut follows the leg and position (and whatever changed under it).
  $effect(() => {
    void cave.version;
    void cave.adjusted;
    void filter.hiddenTrips.length;
    const s = shot;
    const at = t;
    if (!ready) return;
    untrack(() => {
      section = s ? engine.sectionCut(cave.adjusted, s, at, true, wireFilter()) : null;
      if (leg) dist = at * leg.length;
      schedule();
    });
  });

  // Typing a distance moves the position.
  $effect(() => {
    const d = dist;
    untrack(() => {
      if (d !== null && leg && Math.abs(d / leg.length - t) > 1e-9) t = Math.min(1, Math.max(0, d / leg.length));
    });
  });

  $effect(() => {
    void cave.units;
    void barH;
    void width;
    void height;
    schedule();
  });

  function schedule() {
    if (!frame) frame = requestAnimationFrame(() => ((frame = 0), draw()));
  }

  const css = () => {
    const c = getComputedStyle(document.documentElement);
    const v = (n: string) => c.getPropertyValue(n).trim();
    return { text: v('--text'), muted: v('--muted'), line: v('--line'), accent: v('--accent'), accentBg: v('--accent-bg') };
  };

  /** Largest distance from the axis of anything that can be drawn for this leg: a stable scale while sliding. */
  const reach = (s: Section | null) => {
    if (!s) return 1;
    const all = [...s.outline, ...s.from_points, ...s.to_points];
    return Math.max(1, ...all.flatMap((p) => [Math.abs(p[0]), Math.abs(p[1])]));
  };
  let stable = { shot: '', reach: 1 };

  function draw() {
    if (!canvas || width === 0) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    const g = canvas.getContext('2d')!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.clearRect(0, 0, width, height);
    const c = css();
    const s = section;
    if (!s) return;
    if (stable.shot !== s.shot || reach(s) > stable.reach) stable = { shot: s.shot, reach: Math.max(reach(s), stable.shot === s.shot ? stable.reach : 0) };
    const top = barH + 16; // room for the control bar
    const cx = width / 2;
    const cy = top + (height - top - 40) / 2;
    const px = Math.max(5, Math.min(width / 2 - 24, (height - top - 40) / 2 - 8)) / stable.reach; // pixels per metre
    const at = (p: [number, number]): [number, number] => [cx + p[0] * px, cy - p[1] * px];

    // axis cross
    g.strokeStyle = c.line;
    g.lineWidth = 1;
    g.beginPath();
    g.moveTo(cx - 12, cy);
    g.lineTo(cx + 12, cy);
    g.moveTo(cx, cy - 12);
    g.lineTo(cx, cy + 12);
    g.stroke();

    // measured points at the two stations (the evidence the outline is made from)
    const dots = (pts: [number, number][], color: string, hollow: boolean, strong: boolean) => {
      g.globalAlpha = strong ? 1 : 0.55;
      g.strokeStyle = color;
      g.fillStyle = color;
      g.lineWidth = 1.5;
      for (const p of pts) {
        const [x, y] = at(p);
        g.beginPath();
        if (hollow) g.rect(x - 3.5, y - 3.5, 7, 7);
        else g.arc(x, y, 3.5, 0, Math.PI * 2);
        if (hollow) g.stroke();
        else g.fill();
      }
      g.globalAlpha = 1;
    };
    const nearStart = s.t <= 0.5;
    dots(s.to_points, c.muted, true, !nearStart || s.at_station);
    dots(s.from_points, c.accent, false, nearStart || s.at_station);

    if (s.outline.length >= 3) {
      g.beginPath();
      s.outline.forEach((p, i) => {
        const [x, y] = at(p);
        if (i === 0) g.moveTo(x, y);
        else g.lineTo(x, y);
      });
      g.closePath();
      g.fillStyle = c.accentBg;
      g.globalAlpha = 0.55;
      g.fill();
      g.globalAlpha = 1;
      g.strokeStyle = c.text;
      g.lineWidth = 2.2;
      g.lineJoin = 'round';
      g.stroke();
    }

    // up arrow and what it means
    g.fillStyle = c.muted;
    g.strokeStyle = c.muted;
    g.font = '11px system-ui, sans-serif';
    g.textAlign = 'left';
    g.lineWidth = 1.5;
    const ay = height - 52; // bottom of the arrow, above the caption
    g.beginPath();
    g.moveTo(width - 28, ay);
    g.lineTo(width - 28, ay - 26);
    g.moveTo(width - 33, ay - 20);
    g.lineTo(width - 28, ay - 26);
    g.lineTo(width - 23, ay - 20);
    g.stroke();
    g.textAlign = 'right';
    g.fillText(s.up_means === 'north' ? 'north' : 'up', width - 38, ay - 10);

    // legend: which marks are measured at which end
    g.textAlign = 'left';
    g.fillStyle = c.accent;
    g.beginPath();
    g.arc(20, height - 62, 3.5, 0, Math.PI * 2);
    g.fill();
    g.fillStyle = c.muted;
    g.fillText(`measured at ${s.from}`, 30, height - 58);
    g.strokeStyle = c.muted;
    g.lineWidth = 1.5;
    g.strokeRect(16.5, height - 82.5, 7, 7);
    g.fillText(`measured at ${s.to}`, 30, height - 75);

    // scale bar
    const per = metresPer(cave.units);
    const len = niceNumber(100 / px / per);
    g.strokeStyle = c.text;
    g.fillStyle = c.text;
    g.lineWidth = 2;
    g.textAlign = 'left';
    g.beginPath();
    g.moveTo(16, height - 36);
    g.lineTo(16 + len * per * px, height - 36);
    g.stroke();
    g.fillText(`${len} ${unitName(cave.units)}`, 16, height - 42);
  }

  const caption = $derived.by(() => {
    const s = section;
    if (!s) return '';
    const u = cave.units;
    const place = s.at_station
      ? `At station ${s.nearest}: its own measured outline.`
      : `${formatLength(s.nearest_distance, u, 1)} from ${s.nearest}, blended from the walls measured at ${s.from} and ${s.to}.`;
    const infer = [s.from_inferred ? s.from : '', s.to_inferred ? s.to : ''].filter(Boolean);
    const inferred = infer.length ? ` ${infer.join(' and ')} had no measurements, so ${infer.length > 1 ? 'those sections are' : 'that section is'} inferred from neighbouring stations.` : '';
    return `${place}${inferred}`;
  });

  onMount(() => {
    const ro = new ResizeObserver(() => {
      width = wrap.clientWidth;
      height = wrap.clientHeight;
    });
    ro.observe(wrap);
    const dark = window.matchMedia('(prefers-color-scheme: dark)');
    dark.addEventListener('change', schedule);
    loadEngine().then(() => (ready = true));
    return () => {
      ro.disconnect();
      dark.removeEventListener('change', schedule);
      cancelAnimationFrame(frame);
    };
  });
</script>

<div class="section" bind:this={wrap}>
  <canvas bind:this={canvas} style="width:{width}px;height:{height}px"></canvas>
  <div class="bar" bind:clientHeight={barH}>
    <select bind:value={shot} onchange={() => (t = 1)} aria-label="Leg">
      {#each legs as l}<option value={l.shot}>{l.from} – {l.to} · {formatLength(l.length, cave.units, 1)}</option>{/each}
    </select>
    <input type="range" min="0" max="1" step="0.005" bind:value={t} aria-label="Position along the leg" disabled={!leg} />
    {#if leg}<LengthField id="section-distance" label="From {section?.from ?? leg.from}" bind:value={dist} min={0} />{/if}
  </div>
  {#if !legs.length && ready}
    <div class="empty">No legs to cut a section across yet.</div>
  {:else if section?.note}
    <div class="note warn" role="status">{section.note}</div>
  {:else if caption}
    <div class="note">{caption}</div>
  {/if}
</div>

<style>
  .section { position: relative; height: 100%; overflow: hidden; background: var(--panel-2); }
  canvas { position: absolute; inset: 0; display: block; }
  .bar { position: absolute; top: 8px; left: 8px; right: 8px; display: flex; gap: 10px; align-items: center; flex-wrap: wrap; padding: 6px 8px; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); font-size: 12px; width: fit-content; max-width: calc(100% - 16px); }
  select { font: inherit; padding: 3px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); color: var(--text); max-width: 220px; }
  input[type='range'] { width: 140px; accent-color: var(--accent); }
  .note { position: absolute; left: 8px; right: 8px; bottom: 8px; padding: 6px 10px; font-size: 12px; color: var(--muted); background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); }
  .note.warn { color: var(--text); border-color: var(--warn); }
  .empty { position: absolute; inset: 0; display: grid; place-items: center; color: var(--muted); font-size: 13px; }
</style>
