<script lang="ts">
  // The Rooms panel: everything about keeping caves in step through the CaveCAD server, in one place. This
  // computer (signed in, its code), the open cave's sync state, every cave of yours on the server with an Open
  // button, your computers, and Send keys now (gives your other computers the cave keys this one has, so they can
  // open those caves). It reads; it never changes a cave except by opening one.
  import { RefreshCw, KeyRound } from '@lucide/svelte';
  import { account } from '../lib/account.svelte';
  import { cave } from '../lib/cave.svelte';
  import { loadEngine } from '../lib/engine';
  import { ROOM_TEXT, room } from '../lib/room.svelte';
  import { shelf } from '../lib/shelf.svelte';
  import { openServerCave, refreshServerCaves, sendKeysNow, serverCaves } from '../lib/server-caves.svelte';

  let message = $state('');
  let busy = $state(false);

  const signedIn = $derived((account.phase === 'ready' || account.phase === 'offline') && !!account.member);

  // Read now and every 15 s while the panel is showing (signing in reads it again).
  $effect(() => {
    void account.phase;
    void account.member?.id;
    void loadEngine().then(() => refreshServerCaves());
    const t = setInterval(() => void refreshServerCaves(), 15_000);
    return () => clearInterval(t);
  });

  async function refresh() {
    busy = true;
    await refreshServerCaves();
    busy = false;
  }

  async function open(id: string) {
    message = await openServerCave(id);
  }

  const openNowId = $derived(shelf.records.find((r) => r.id === shelf.currentId)?.roomId ?? '');
  const here = $derived(serverCaves.computers.find((c) => c.here) ?? null);

  const date = (ms: number) => (ms ? new Date(ms).toLocaleDateString(undefined, { dateStyle: 'medium' }) : '—');
  function ago(ms: number | null): string {
    if (!ms) return 'never';
    const s = Math.max(0, (Date.now() - ms) / 1000);
    if (s < 90) return 'just now';
    if (s < 3600) return `${Math.round(s / 60)} min ago`;
    if (s < 86400 * 2) return `${Math.round(s / 3600)} h ago`;
    return date(ms);
  }

  /** What one cave is to this computer, in words. */
  function where(c: (typeof serverCaves.all)[number]): string {
    if (c.id === openNowId && cave.hasCave) return 'open now';
    if (c.shelfId) return 'on this computer';
    if (serverCaves.opening === c.id) return 'fetching…';
    if (!c.hasKey) return serverCaves.waitingFor === c.id ? 'waiting for a key: opens by itself when it arrives' : 'waiting for a key';
    return 'not on this computer yet';
  }

  /** The open cave's state in words, including why a cave does not sync at all. */
  const openState = $derived(
    !cave.hasCave
      ? 'No cave is open.'
      : room.info
        ? room.info
        : room.status
          ? ROOM_TEXT[room.status] + (room.status === 'error' && room.reason ? ` (${room.reason})` : '')
          : !signedIn
            ? 'Not signed in: this cave is kept only on this computer.'
            : 'Not syncing: samples never sync, and a cave stops while a live session or class runs.',
  );
</script>

<div class="panel">
  <div class="bar">
    <button onclick={refresh} disabled={busy || !signedIn} title="Read the list from the server again"><RefreshCw size={14} /> Refresh</button>
    <button onclick={() => void sendKeysNow()} disabled={!signedIn} title="Give your other computers the keys this one has, so they can open those caves"><KeyRound size={14} /> Send keys now</button>
    {#if serverCaves.sent}<span class="note">{serverCaves.sent}</span>{/if}
  </div>

  <div class="body">
    <section>
      <h3>This computer</h3>
      {#if !signedIn}
        <p>Not signed in. Sign in (top right) to keep your caves in step across your computers.</p>
      {:else}
        <p>Signed in as <b>{account.member?.displayName}</b> ({account.member?.email}){account.phase === 'offline' ? ' · offline' : ''}</p>
        {#if here}
          <p class="muted">Computer code <span class="code">{here.code}</span> · added {date(here.createdAt)}</p>
        {:else if serverCaves.loaded}
          <p class="muted">This computer is not registered with the server yet.</p>
        {/if}
      {/if}
      {#if serverCaves.problem}<p class="err">{serverCaves.problem}</p>{/if}
    </section>

    <section>
      <h3>Open cave</h3>
      <p>{#if cave.hasCave}<b class="cave">{cave.name}</b>{/if}{openState}</p>
    </section>

    <section>
      <h3>Your caves on the server</h3>
      {#if !signedIn}
        <p class="muted">Sign in to see them.</p>
      {:else if !serverCaves.loaded}
        <p class="muted">Reading…</p>
      {:else if serverCaves.all.length === 0}
        <p class="muted">None yet. A cave you make or open while signed in is kept here (samples are not).</p>
      {:else}
        <table>
          <thead><tr><th>Cave</th><th>Here</th><th>Made</th><th></th></tr></thead>
          <tbody>
            {#each serverCaves.all as c (c.id)}
              <tr>
                <td>{c.name ?? 'A cave from another computer'}</td>
                <td class:wait={!c.hasKey && !c.shelfId}>{where(c)}</td>
                <td class="muted">{date(c.createdAt)}</td>
                <td class="act">
                  <button onclick={() => void open(c.id)} disabled={(c.id === openNowId && cave.hasCave) || !!serverCaves.opening || serverCaves.waitingFor === c.id}>{serverCaves.opening === c.id ? 'Opening…' : 'Open'}</button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
        {#if serverCaves.all.some((c) => !c.hasKey && !c.shelfId)}
          <p class="muted">A cave waiting for a key opens once one of your other computers runs CaveCAD and hands its key on (or press Send keys now there).</p>
        {/if}
      {/if}
      {#if message}<p class="err">{message}</p>{/if}
    </section>

    <section>
      <h3>Your computers</h3>
      {#if serverCaves.computers.length === 0}
        <p class="muted">{signedIn ? 'Reading…' : 'Sign in to see them.'}</p>
      {:else}
        <table>
          <thead><tr><th>Computer</th><th>Code</th><th>Added</th><th>Last seen</th></tr></thead>
          <tbody>
            {#each serverCaves.computers as d, i (d.id)}
              <tr>
                <td>{d.here ? 'This computer' : `Computer ${i + 1}`}</td>
                <td class="code">{d.code}</td>
                <td class="muted">{date(d.createdAt)}</td>
                <td class="muted">{d.here ? 'now' : ago(d.lastSeen)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
        <p class="muted">Each browser counts as a computer: it keeps its own key. The code is shown on that computer's own Rooms panel, so you can tell which is which.</p>
      {/if}
    </section>
  </div>
</div>

<style>
  .panel { display: flex; flex-direction: column; height: 100%; background: var(--panel); color: var(--text); font-size: 13px; }
  .bar { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; padding: 6px 10px; border-bottom: 1px solid var(--line); }
  .bar button, .act button { display: inline-flex; align-items: center; gap: 5px; font: inherit; font-size: 12px; padding: 4px 10px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  .body { flex: 1; overflow: auto; padding: 4px 12px 12px; }
  section { padding: 8px 0; border-bottom: 1px solid var(--line); }
  section:last-child { border-bottom: none; }
  h3 { margin: 0 0 4px; font-size: 12px; font-weight: 600; color: var(--muted); text-transform: uppercase; letter-spacing: 0.04em; }
  p { margin: 2px 0; }
  .muted, .note { color: var(--muted); font-size: 12px; }
  .err { color: var(--danger); font-size: 12px; }
  .code { font-family: ui-monospace, monospace; font-size: 12px; white-space: nowrap; }
  .cave { margin-right: 8px; }
  table { border-collapse: collapse; width: 100%; font-size: 12px; margin-top: 4px; }
  th { text-align: left; color: var(--muted); font-weight: 600; padding: 3px 6px; border-bottom: 1px solid var(--line); }
  td { padding: 4px 6px; border-bottom: 1px solid var(--line); }
  td.wait { color: var(--accent); }
  .act { text-align: right; width: 1%; white-space: nowrap; }
</style>
