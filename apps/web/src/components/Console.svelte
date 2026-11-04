<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { engine, type ConsoleEvent, type Level } from '../lib/engine';

  const order: Level[] = ['debug', 'detail', 'info', 'warn', 'error'];
  let events = $state<ConsoleEvent[]>([]);
  let min = $state<Level>('info');
  let query = $state('');
  let follow = $state(true);
  let box: HTMLDivElement;
  let next = 0;

  const shown = $derived(
    events.filter(
      (e) => order.indexOf(e.level) >= order.indexOf(min) && (query === '' || `${e.source} ${e.message}`.toLowerCase().includes(query.toLowerCase())),
    ),
  );

  // The same line said again and again (each redraw, say) is shown once, with how many times.
  const rows = $derived.by(() => {
    const out: { e: ConsoleEvent; times: number }[] = [];
    for (const e of shown) {
      const last = out[out.length - 1];
      if (last && last.e.level === e.level && last.e.source === e.source && last.e.message === e.message) last.times += 1;
      else out.push({ e, times: 1 });
    }
    return out;
  });

  onMount(() => {
    const timer = setInterval(async () => {
      let fresh: ConsoleEvent[] = [];
      try {
        fresh = engine.events(next);
      } catch {
        return; // engine not loaded yet
      }
      if (fresh.length === 0) return;
      next = fresh[fresh.length - 1].seq + 1;
      events = [...events, ...fresh].slice(-5000);
      if (follow) {
        await tick();
        box.scrollTop = box.scrollHeight;
      }
    }, 250);
    return () => clearInterval(timer);
  });

  const copy = () => navigator.clipboard.writeText(rows.map(({ e, times }) => `${e.level.toUpperCase()} ${e.source}: ${e.message}${times > 1 ? ` (×${times})` : ''}`).join('\n'));
  const clear = () => {
    engine.clear();
    events = [];
  };
  const onScroll = () => (follow = box.scrollTop + box.clientHeight >= box.scrollHeight - 4);
</script>

<section>
  <div class="tools">
    <select bind:value={min} aria-label="Minimum level">
      <option value="info">Info</option>
      <option value="detail">Detail</option>
      <option value="debug">Debug</option>
      <option value="warn">Warnings</option>
      <option value="error">Errors</option>
    </select>
    <input placeholder="Search" bind:value={query} />
    <span class="spacer"></span>
    <button onclick={copy}>Copy</button>
    <button onclick={clear}>Clear</button>
  </div>
  <div class="log" bind:this={box} onscroll={onScroll} role="log">
    {#each rows as { e, times } (e.seq)}
      <div class={e.level}><span class="src">{e.source}</span> {e.message}{#if times > 1} <span class="times" title="Said {times} times in a row">×{times}</span>{/if}</div>
    {/each}
  </div>
</section>

<style>
  section { display: grid; grid-template-rows: auto 1fr; background: var(--panel); height: 100%; min-height: 0; }
  .tools { display: flex; align-items: center; gap: 8px; padding: 6px 10px; border-bottom: 1px solid var(--line); }
  .spacer { flex: 1; }
  select, input, .tools button { font: inherit; padding: 3px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel); color: var(--text); }
  .log { overflow: auto; padding: 6px 10px; font-family: var(--mono); font-size: 12px; line-height: 1.6; user-select: text; }
  .src { color: var(--muted); display: inline-block; min-width: 64px; }
  .warn { color: var(--warn); }
  .error { color: var(--danger); }
  .detail, .debug { color: var(--muted); }
  .times { color: var(--muted); font-size: 11px; }
</style>
