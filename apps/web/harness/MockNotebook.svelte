<script lang="ts">
  // What the mock notebook holds, as a table: the harness's stand-in for the Notebook panel, so you can
  // watch each tap on the Survey panel land. Dockable like any panel.
  import { untrack } from 'svelte';
  import { cave } from '../src/lib/cave.svelte';

  type Shot = { id: string; from: string; to: string | null; c: Record<string, number | null> };
  let shots = $state<Shot[]>([]);

  $effect(() => {
    void cave.version;
    untrack(() => {
      const book = (globalThis as unknown as { __book?: { shots: Shot[] } }).__book;
      shots = structuredClone($state.snapshot(book?.shots ?? []));
    });
  });

  const f = (v: number | null | undefined, d = 2) => (v === null || v === undefined ? '' : (+v.toFixed(d)).toString());
</script>

<div class="book" aria-label="Mock notebook">
  <table>
    <thead><tr><th>From</th><th>To</th><th>Tape (m)</th><th>Az fs</th><th>Az bs</th><th>Inc fs</th><th>Inc bs</th></tr></thead>
    <tbody>
      {#each shots as s (s.id)}
        <tr class:splay={!s.to}>
          <td>{s.from}</td><td>{s.to ?? 'splay'}</td><td>{f(s.c.distance, 3)}</td><td>{f(s.c.az_fs)}</td><td>{f(s.c.az_bs)}</td><td>{f(s.c.inc_fs)}</td><td>{f(s.c.inc_bs)}</td>
        </tr>
      {:else}
        <tr><td colspan="7" class="none">Nothing written yet. Press Simulator, then Take shot.</td></tr>
      {/each}
    </tbody>
  </table>
</div>

<style>
  .book { height: 100%; overflow: auto; padding: 8px 10px; box-sizing: border-box; background: var(--panel); color: var(--text); font-size: 13px; }
  table { border-collapse: collapse; width: 100%; font-variant-numeric: tabular-nums; }
  th { text-align: left; color: var(--muted); font-weight: 600; font-size: 11px; padding: 3px 6px; border-bottom: 1px solid var(--line); position: sticky; top: 0; background: var(--panel); }
  td { padding: 3px 6px; border-bottom: 1px solid var(--line); }
  tr.splay td { color: var(--muted); }
  .none { color: var(--muted); padding: 14px 6px; }
</style>
