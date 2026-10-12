<script lang="ts">
  // The Cave Shelf: the window CaveCAD opens on. The caves this browser keeps are on the left (with a
  // few samples below); the one picked shows on the right: how big it is, where each trip started and
  // stopped, and the open ends where a new trip could pick up. Open it, add a trip to it, start a new
  // cave, or bring one in from a file. Nothing here changes a cave except by opening it.
  import { untrack } from 'svelte';
  import { Copy, Star } from '@lucide/svelte';
  import { joinWithCode, leaveP2p, p2p } from '../lib/p2p.svelte';
  import { live, me, rename } from '../lib/sync.svelte';
  import { cave } from '../lib/cave.svelte';
  import { DEMOS } from '../lib/demos';
  import { engine, loadEngine, type CaveSummary } from '../lib/engine';
  import { openDxf } from '../lib/files';
  import { openAtTrip, newTripOn, newTripOnSample, openKept, openSample } from '../lib/caves';
  import { dxfOf, forgetCave, loadShelf, setFavorite, setStartup, shelf, summaryOf, type ShelfRecord } from '../lib/shelf.svelte';
  import { formatLength } from '../lib/units';

  let search = $state('');
  /** The cave picked on the left: a record id, `sample:<id>`, or ''. */
  let picked = $state('');
  let summary = $state<CaveSummary | null>(null);
  /** The open end the next trip would start from. */
  let from = $state('');
  let message = $state('');
  let menu = $state<{ id: string; x: number; y: number } | null>(null);
  /** Joining a session in progress: the host's invite code pasted, and the reply being made. */
  let joining = $state(false);
  let inviteText = $state('');
  let joinProblem = $state('');
  let copiedReply = $state(false);

  async function makeReply() {
    joinProblem = (await joinWithCode(inviteText)) ?? '';
  }

  async function copyReply() {
    try {
      await navigator.clipboard.writeText(p2p.reply);
      copiedReply = true;
      setTimeout(() => (copiedReply = false), 1500);
    } catch {
      /* selectable: copy it by hand */
    }
  }

  function startJoin() {
    joining = true;
    joinProblem = '';
    p2p.error = '';
  }

  function cancelJoin() {
    leaveP2p();
    joining = false;
    inviteText = '';
    joinProblem = '';
  }

  $effect(() => {
    if (shelf.open) {
      void loadEngine().then(() => loadShelf());
      message = '';
    }
  });

  const shown = $derived(shelf.records.filter((r) => r.name.toLowerCase().includes(search.trim().toLowerCase())));
  const record = $derived(shelf.records.find((r) => r.id === picked) ?? null);
  const sample = $derived(picked.startsWith('sample:') ? (DEMOS.find((d) => `sample:${d.id}` === picked) ?? null) : null);

  // Start on the cave in use, else the first one.
  $effect(() => {
    if (!shelf.open) return;
    untrack(() => {
      if (!picked || (!record && !sample)) picked = shelf.records.find((r) => r.id === shelf.currentId)?.id ?? shelf.records[0]?.id ?? `sample:${DEMOS[0].id}`;
    });
  });

  // What the cave on the right says about itself: kept with the record, read once if it has none (a
  // sample is read from its own file).
  $effect(() => {
    const rec = record;
    const smp = sample;
    void loadEngine().then(() => {
      untrack(() => {
        summary = rec?.summary ?? (smp ? engine.shelfSummarizeDemo(smp.json) : null);
        from = '';
        if (rec && !rec.summary) void summaryOf(rec).then((s) => rec.id === picked && (summary = s));
      });
    });
  });

  // An open end to start from: the newest by default.
  $effect(() => {
    const s = summary;
    untrack(() => {
      if (s && !from) from = s.ends[0]?.station ?? '';
    });
  });

  const length = (m: number) => formatLength(m, cave.units, 0);
  const when = (ms: number) => new Date(ms).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
  const health = (s: CaveSummary) => (s.empty ? 'nothing surveyed yet' : s.worst_percent === null ? 'no loops closed' : `worst loop ${s.worst_percent.toFixed(1)}%`);

  async function run(job: Promise<string | null>) {
    const problem = await job;
    message = problem ?? '';
  }

  const open = () => (record ? run(openKept(record.id)) : sample ? run(openSample(sample.id)) : undefined);
  const newTrip = () => (record ? run(newTripOn(record.id, from)) : sample ? run(newTripOnSample(sample.id, from)) : undefined);
  const atTrip = (index: number) => record && run(openAtTrip(record.id, index));

  async function importFile() {
    const r = await openDxf();
    if (r && r.ok) shelf.open = false;
    else if (r) message = r.message;
  }

  async function download(id: string) {
    const text = await dxfOf(id);
    const rec = shelf.records.find((r) => r.id === id);
    if (!text || !rec) return;
    const url = URL.createObjectURL(new Blob([text], { type: 'application/dxf' }));
    const a = document.createElement('a');
    a.href = url;
    a.download = `${rec.name}.dxf`;
    a.click();
    URL.revokeObjectURL(url);
  }

  async function forget(id: string) {
    const rec = shelf.records.find((r) => r.id === id);
    if (!rec || !window.confirm(`Take “${rec.name}” off this shelf? The copy this browser keeps is deleted. A file you saved is not touched.`)) return;
    await forgetCave(id);
    if (picked === id) picked = '';
  }

  function rightClick(e: MouseEvent, id: string) {
    e.preventDefault();
    picked = id;
    menu = { id, x: Math.min(e.clientX, window.innerWidth - 220), y: Math.min(e.clientY, window.innerHeight - 200) };
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') (menu ? (menu = null) : (shelf.open = false));
    else if (e.key === 'Enter' && !joining && !(e.target as HTMLElement).closest('input,button,textarea')) void open();
  }

  const close = () => (shelf.open = false);
</script>

{#if shelf.open}
  <div class="veil" role="presentation" onclick={close}>
    <div class="box" class:joinbox={joining} role="dialog" aria-modal="true" aria-label={joining ? 'Join a live session' : 'Caves'} tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={key}>
      {#if joining}
        <div class="joinpage">
          <h2>Join a session in progress</h2>
          <p class="stats">Someone is sharing a cave live. Paste the invite code they sent you; you get a reply code to send back; when they paste it, their cave opens here and you work on it together. Nothing goes through a server.</p>
          <label class="field">Your name <input value={me.name} maxlength="40" onchange={(e) => rename(e.currentTarget.value)} /></label>
          <p class="step">1. Paste the host’s invite code.</p>
          <textarea rows="3" placeholder="CAVE1-…" bind:value={inviteText} disabled={p2p.role === 'guest'} aria-label="Invite code from the host"></textarea>
          {#if p2p.role !== 'guest'}
            <div><button class="go" onclick={makeReply} disabled={!inviteText.trim()}>Make my reply code</button></div>
          {:else}
            <p class="step">2. Send this reply code back to the host.</p>
            <div class="replybox">
              <textarea readonly rows="3" value={p2p.reply || 'Making the code…'} aria-label="Your reply code" onfocus={(e) => e.currentTarget.select()}></textarea>
              <button onclick={copyReply} disabled={!p2p.reply}><Copy size={13} /> {copiedReply ? 'Copied' : 'Copy'}</button>
            </div>
            <p class="stats" role="status">{live.status === 'live' ? 'You are in.' : p2p.waiting || 'Waiting for the host…'}</p>
          {/if}
          <label class="check"><input type="checkbox" bind:checked={p2p.following} /> Follow what the host is looking at</label>
          <label class="check" title="A public STUN service tells each device its public address so they can find each other across networks. No cave data goes through it."><input type="checkbox" bind:checked={p2p.stun} disabled={p2p.role === 'guest'} /> Connect across the internet</label>
          {#if joinProblem}<p class="err" role="alert">{joinProblem}</p>{/if}
          {#if p2p.error}<p class="err" role="alert">{p2p.error}</p>{/if}
          <div class="spacer"></div>
          <div class="foot"><span class="grow"></span><button onclick={cancelJoin}>Back to my caves</button></div>
        </div>
      {:else}
      <div class="rail">
        <input type="search" placeholder="Search caves" bind:value={search} aria-label="Search caves" />
        <div class="list" role="listbox" aria-label="Caves">
          {#each shown as r (r.id)}
            <button class="cave" class:sel={picked === r.id} role="option" aria-selected={picked === r.id} onclick={() => (picked = r.id)} ondblclick={open} oncontextmenu={(e) => rightClick(e, r.id)}>
              <span class="name">{#if r.favorite}<Star size={12} fill="currentColor" />{/if} {r.name}</span>
              <span class="sub">{r.summary ? (r.summary.empty ? 'nothing surveyed yet' : `${r.summary.trips.length} trip${r.summary.trips.length === 1 ? '' : 's'} · ${length(r.summary.length_m)}`) : '…'}{r.id === shelf.currentId ? ' · open now' : ''}</span>
            </button>
          {:else}
            <p class="none">{shelf.records.length ? 'No cave matches.' : 'No caves yet. Start one, or bring one in from a file.'}</p>
          {/each}
          <div class="group">Samples</div>
          {#each DEMOS as d}
            <button class="cave" class:sel={picked === `sample:${d.id}`} onclick={() => (picked = `sample:${d.id}`)} ondblclick={open}>
              <span class="name">{d.title.replace(/ \(.*\)$/, '')}</span>
              <span class="sub">sample cave</span>
            </button>
          {/each}
        </div>
        <div class="row">
          <button onclick={() => { shelf.open = false; cave.newDialog = true; }}>New cave…</button>
          <button onclick={importFile} title="A cave saved as a DXF file by CaveCAD">Import file…</button>
        </div>
        <button class="join" class:sel={joining} onclick={startJoin} title="Someone is sharing a cave live: paste the invite code they sent you">Join a live session…</button>
      </div>

      <div class="detail">
        {#if (record || sample) && summary}
          <h2>{summary.name}</h2>
          <p class="stats">
            {#if summary.empty}Nothing surveyed yet.{:else}{length(summary.length_m)} surveyed · {summary.stations} stations · {summary.trips.length} trip{summary.trips.length === 1 ? '' : 's'}{#if summary.depth_m !== null && summary.depth_m > 0} · {length(summary.depth_m)} deep{/if} · {health(summary)}{/if}
          </p>
          {#if record}<p class="stored">Kept in this browser · last saved {when(record.updated)}</p>{:else}<p class="stored">A sample cave. Open it to look around, or start a trip on it (it is then kept in this browser as your own copy).</p>{/if}
          {#if summary.ends.length}
            <div class="ends">
              <span class="lead">Open ends</span>
              {#each summary.ends as e}
                <button class="chip" class:on={from === e.station} title="Start the next trip here" onclick={() => (from = from === e.station ? '' : e.station)}>{e.station}</button>
              {/each}
            </div>
          {/if}
          <div class="table">
            <table>
              <thead><tr><th>Trip</th><th>Date</th><th>Decl</th><th>Team</th><th>Shots</th><th>From</th><th>To</th><th>Ends at</th></tr></thead>
              <tbody>
                {#each summary.trips as t}
                  <tr ondblclick={() => atTrip(t.index)} title="Double-click to open the cave at this trip">
                    <td>{t.name || 'Untitled trip'}</td><td>{t.date}</td><td class:grey={t.declination === null}>{t.declination === null ? '—' : `${t.declination > 0 ? '+' : ''}${t.declination}°`}</td><td>{t.team}</td><td class="num">{t.shots}</td><td>{t.start || '—'}</td><td>{t.end || '—'}</td><td>{t.ends.join(', ') || '—'}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        {:else}
          <p class="none big">Pick a cave on the left, or start a new one.</p>
        {/if}
        {#if shelf.problem}<p class="err" role="alert">{shelf.problem}</p>{/if}
        {#if message}<p class="err" role="alert">{message}</p>{/if}
        <div class="spacer"></div>
        <div class="foot">
          <label><input type="checkbox" checked={shelf.startup} onchange={(e) => setStartup(e.currentTarget.checked)} /> Open this window at startup</label>
          <span class="grow"></span>
          <button onclick={newTrip} disabled={(!record && !sample) || !summary} title="Open the cave and start a new trip on it">{from ? `New trip from ${from}` : 'New trip'}</button>
          <button class="go" onclick={open} disabled={!record && !sample}>Open</button>
          <button onclick={close}>{cave.hasCave ? 'Close' : 'Start empty'}</button>
        </div>
      </div>
      {/if}
    </div>
  </div>

  {#if menu}
    {@const rec = shelf.records.find((r) => r.id === menu!.id)}
    <div class="veil2" role="presentation" onclick={() => (menu = null)} oncontextmenu={(e) => { e.preventDefault(); menu = null; }}></div>
    <div class="menu" role="menu" style="left: {menu.x}px; top: {menu.y}px">
      <button role="menuitem" onclick={() => { menu = null; void open(); }}>Open</button>
      <button role="menuitem" onclick={() => { menu = null; void newTrip(); }}>{from ? `New trip from ${from}` : 'New trip'}</button>
      <hr />
      <button role="menuitem" onclick={() => { const id = menu!.id; menu = null; void setFavorite(id, !rec?.favorite); }}>{rec?.favorite ? 'Unfavorite' : 'Favorite ★'}</button>
      <button role="menuitem" onclick={() => { const id = menu!.id; menu = null; void download(id); }}>Download a file…</button>
      <hr />
      <button role="menuitem" onclick={() => { const id = menu!.id; menu = null; void forget(id); }}>Forget…</button>
    </div>
  {/if}
{/if}

<style>
  .veil { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); display: grid; place-items: center; z-index: 40; }
  .box { width: min(980px, 96vw); height: min(600px, 92vh); background: var(--panel); color: var(--text); border: 1px solid var(--line); border-radius: 10px; display: flex; outline: none; overflow: hidden; }
  .rail { width: 290px; flex: none; border-right: 1px solid var(--line); display: flex; flex-direction: column; gap: 8px; padding: 12px; }
  .list { flex: 1; overflow: auto; display: flex; flex-direction: column; gap: 2px; }
  button.join { margin-top: 2px; }
  button.join.sel { background: var(--accent-bg); border-color: var(--accent); }
  .field { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--muted); }
  .field input { font: inherit; font-size: 13px; padding: 5px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); }
  .step { margin: 0; font-size: 12px; }
  .replybox { display: flex; gap: 6px; align-items: flex-start; }
  .check { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--muted); }
  textarea { font: 11px/1.3 var(--mono, monospace); padding: 6px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); resize: vertical; word-break: break-all; flex: 1; }
  .group { font-size: 11px; font-weight: 600; color: var(--muted); padding: 10px 6px 2px; }
  .cave { text-align: left; display: flex; flex-direction: column; gap: 1px; padding: 6px 8px; border: none; background: none; border-radius: 6px; color: var(--text); cursor: pointer; font: inherit; }
  .cave:hover { background: var(--panel-2); }
  .cave.sel { background: var(--accent-bg); }
  .cave .name { font-weight: 600; font-size: 13px; display: flex; align-items: center; gap: 4px; }
  .cave .sub { font-size: 11px; color: var(--muted); }
  .row { display: flex; gap: 6px; }
  .row button { flex: 1; }
  .joinbox { width: min(640px, 96vw); height: auto; max-height: 92vh; }
  .joinpage { flex: 1; min-width: 0; padding: 20px 24px; display: flex; flex-direction: column; gap: 10px; overflow: auto; }
  .detail { flex: 1; min-width: 0; padding: 16px 20px; display: flex; flex-direction: column; gap: 8px; }
  h2 { margin: 0; font-size: 22px; }
  .stats { margin: 0; color: var(--text); font-size: 13px; }
  .stored { margin: 0; color: var(--muted); font-size: 11px; }
  .ends { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .lead { font-size: 12px; color: var(--muted); }
  .chip { padding: 2px 9px; border-radius: 999px; border: 1px solid var(--line); background: none; color: var(--text); font: inherit; font-size: 12px; cursor: pointer; }
  .chip.on { background: var(--accent-bg); border-color: var(--accent); }
  .table { flex: 1; min-height: 80px; overflow: auto; border: 1px solid var(--line); border-radius: 8px; }
  table { border-collapse: collapse; width: 100%; font-size: 12px; }
  th { position: sticky; top: 0; background: var(--panel); text-align: left; padding: 5px 8px; color: var(--muted); font-weight: 600; border-bottom: 1px solid var(--line); }
  td { padding: 4px 8px; border-bottom: 1px solid var(--line); }
  tbody tr:hover { background: var(--panel-2); cursor: default; }
  .num { text-align: right; }
  .grey { color: var(--muted); }
  .spacer { flex: none; }
  .foot { display: flex; align-items: center; gap: 8px; padding-top: 4px; }
  .foot label { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--muted); }
  .grow { flex: 1; }
  .none { color: var(--muted); font-size: 12px; padding: 8px 6px; }
  .none.big { font-size: 14px; padding: 40px 0; text-align: center; }
  .err { margin: 0; color: var(--danger); font-size: 12px; }
  input[type='search'] { font: inherit; font-size: 13px; padding: 6px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); }
  button { font: inherit; font-size: 13px; padding: 6px 12px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  button.go { background: var(--accent); border-color: var(--accent); color: #fff; }
  .veil2 { position: fixed; inset: 0; z-index: 60; }
  .menu { position: fixed; z-index: 61; min-width: 190px; background: var(--panel); border: 1px solid var(--line); border-radius: 8px; padding: 4px; box-shadow: 0 6px 24px rgba(0, 0, 0, 0.35); display: flex; flex-direction: column; }
  .menu button { border: none; background: none; text-align: left; padding: 6px 8px; border-radius: 4px; }
  .menu button:hover { background: var(--accent-bg); }
  .menu hr { border: none; border-top: 1px solid var(--line); margin: 4px 2px; width: 100%; }
</style>
