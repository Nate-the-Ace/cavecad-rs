<script lang="ts">
  // The Cave Shelf: the window CaveCAD opens on. The caves this browser keeps are on the left (with a
  // few samples below); the one picked shows on the right: how big it is, where each trip started and
  // stopped, and the open ends where a new trip could pick up. Open it, add a trip to it, start a new
  // cave, or bring one in from a file. Nothing here changes a cave except by opening it. Signed in, the caves kept on
  // the CaveCAD server that this browser does not have yet (made on another device) are listed too, under "On your
  // other devices"; opening one fetches it and keeps it here (server-caves.svelte.ts).
  import { untrack } from 'svelte';
  import { Copy, Star } from '@lucide/svelte';
  import { cave } from '../lib/cave.svelte';
  import { DEMOS } from '../lib/demos';
  import { engine, loadEngine, type CaveSummary } from '../lib/engine';
  import { openCave } from '../lib/files';
  import { openAtTrip, newTripOn, newTripOnSample, openKept, openSample } from '../lib/caves';
  import { deleteEverywhere, leaveShared, openServerCave, refreshServerCaves, serverCaves, type OtherCave } from '../lib/server-caves.svelte';
  import { account } from '../lib/account.svelte';
  import { dxfOf, forgetCave, homeOf, loadShelf, setFavorite, setRecordHome, setStartup, shelf, summaryOf, type ShelfRecord } from '../lib/shelf.svelte';
  import { findFolder, reachable, revealHome } from '../lib/cavefolder.svelte';
  import { folderExistsNative, isDesktop } from '../lib/desktop';
  import { formatLength } from '../lib/units';
  import { openContextMenu } from '../lib/context-menu.svelte';

  let search = $state('');
  /** The cave picked on the left: a record id, `sample:<id>`, or ''. */
  let picked = $state('');
  let summary = $state<CaveSummary | null>(null);
  /** The open end the next trip would start from. */
  let from = $state('');
  let message = $state('');
  $effect(() => {
    if (shelf.open) {
      void loadEngine().then(() => loadShelf()).then(() => refreshServerCaves());
      message = '';
    }
  });

  // While open, the caves on the server are read again now and then: a key handed over by another device shows up,
  // and a cave someone asked to open opens by itself once its key is here (server-caves.svelte.ts).
  $effect(() => {
    if (!shelf.open) return;
    void account.phase; // signing in (or out) while the shelf is open reads the list again
    void account.member?.id;
    untrack(() => void loadEngine().then(() => refreshServerCaves()));
    const t = setInterval(() => void refreshServerCaves(), 15_000);
    return () => clearInterval(t);
  });

  const shown = $derived(shelf.records.filter((r) => r.name.toLowerCase().includes(search.trim().toLowerCase())));
  const record = $derived(shelf.records.find((r) => r.id === picked) ?? null);
  const sample = $derived(picked.startsWith('sample:') ? (DEMOS.find((d) => `sample:${d.id}` === picked) ?? null) : null);
  /** A cave on the server this browser does not keep yet. */
  const remote = $derived(picked.startsWith('server:') ? (serverCaves.list.find((c) => `server:${c.id}` === picked) ?? null) : null);
  const remoteShown = $derived(serverCaves.list.filter((c) => (c.name ?? '').toLowerCase().includes(search.trim().toLowerCase()) || (!c.name && !search.trim())));
  const remoteMine = $derived(remoteShown.filter((c) => c.role === 'owner'));
  const remoteShared = $derived(remoteShown.filter((c) => c.role !== 'owner'));
  /** "Nathan's" (and "watching") for a cave shared with this member, or '' for their own. */
  /** Why a cave on the server cannot open here yet, in plain words: it is still on its way from a device that has it. */
  function comingOver(c: { id: string; role: string; ownerName: string }): string {
    const from = c.role === 'owner' ? 'your other device. Open CaveCAD there once' : `${c.ownerName || 'the person who shared it'}. It arrives the next time they have CaveCAD open`;
    return `Still coming over from ${from}${c.role === 'owner' ? ' and it arrives here' : ''}${serverCaves.waitingFor === c.id ? '; it opens by itself.' : '.'}`;
  }

  function whose(c: { role: string; ownerName: string }): string {
    if (c.role === 'owner') return '';
    return `${c.ownerName ? `${c.ownerName}'s` : 'shared with you'}${c.role === 'watcher' ? ' · watching' : ''}`;
  }
  const sharedOf = (shelfId: string) => serverCaves.all.find((c) => c.shelfId === shelfId && c.role !== 'owner');

  // Start on the cave in use, else the first one.
  $effect(() => {
    if (!shelf.open) return;
    untrack(() => {
      if (!picked || (!record && !sample && !remote)) picked = shelf.records.find((r) => r.id === shelf.currentId)?.id ?? shelf.records[0]?.id ?? `sample:${DEMOS[0].id}`;
    });
  });

  // What the cave on the right says about itself: kept with the record, read once if it has none (a
  // sample is read from its own file).
  $effect(() => {
    const rec = record;
    const smp = sample;
    const rem = remote;
    void loadEngine().then(() => {
      untrack(() => {
        summary = rec?.summary ?? (smp ? engine.shelfSummarizeDemo(smp.json) : (rem?.summary ?? null));
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
  const health = (s: CaveSummary) => (s.empty ? 'nothing surveyed yet' : s.worst_percent === null ? 'no survey loops yet' : `worst loop ${s.worst_percent.toFixed(1)}% off`);
  /** What the loop number means, on hover. */
  const HEALTH_HINT = 'Where the survey comes back round to a station it already reached, the two ways never meet exactly. This is how far off the worst loop is, as a share of its length: under about 1% is good; much more usually means a misread or mistyped shot.';

  async function run(job: Promise<string | null>) {
    const problem = await job;
    message = problem ?? '';
  }

  /** The picked cave's folder is not where it was. A browser that has not been allowed to look yet is not asked here. */
  let lost = $state(false);
  $effect(() => {
    const rec = record;
    lost = false;
    if (!rec?.home) return;
    void (async () => {
      const home = await homeOf(rec);
      let gone = !home;
      if (home?.kind === 'path') gone = !isDesktop() || !(await folderExistsNative(home.path));
      else if (home && (await home.handle.queryPermission?.({ mode: 'readwrite' })) === 'granted') gone = !(await reachable(home, false));
      if (record?.id === rec.id) lost = gone;
    })();
  });

  async function find(rec: ShelfRecord) {
    const home = await findFolder(rec.home?.file ?? `${rec.name}.cavecad`).catch((e) => String(e));
    if (home === null) return;
    if (typeof home === 'string') return (message = home);
    await setRecordHome(rec.id, home);
    lost = false;
    message = '';
  }

  async function keepHere(rec: ShelfRecord) {
    if (!window.confirm(`Keep “${rec.name}” in this browser only? Nothing more is written to ${rec.home?.label}; the folder itself is not touched.`)) return;
    await setRecordHome(rec.id, null);
    lost = false;
  }

  async function reveal(rec: ShelfRecord) {
    const home = await homeOf(rec);
    if (!home || !(await revealHome(home).catch(() => false))) message = `That cave's folder is ${rec.home?.label ?? 'not set'}.`;
  }

  /** Fetches a cave from the server; one with no key here yet waits, and opens by itself when the key arrives. */
  const openRemote = async (id: string) => (message = await openServerCave(id));

  const open = () => (record ? run(openKept(record.id)) : sample ? run(openSample(sample.id)) : remote ? openRemote(remote.id) : undefined);
  const newTrip = () => (record ? run(newTripOn(record.id, from)) : sample ? run(newTripOnSample(sample.id, from)) : undefined);
  const atTrip = (index: number) => record && run(openAtTrip(record.id, index));

  async function importFile() {
    const r = await openCave();
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

  /**
   * Removing a cave, one at a time and on purpose: a kept cave can come off this computer only; a cave of the member's
   * own on the server can be deleted everywhere (after typing its name); a cave shared with them can be left.
   */
  let removing = $state<{ rec: ShelfRecord | null; server: OtherCave | null; step: 'choose' | 'delete' } | null>(null);
  let typed = $state('');
  let removeBusy = $state(false);
  let removeProblem = $state('');
  /** The cave on the server for a kept record, when it has one. */
  const onServer = (rec: ShelfRecord | null) => (rec?.roomId ? (serverCaves.all.find((c) => c.id === rec.roomId) ?? null) : null);
  /** What the person types to delete: the cave's name, or DELETE when the server has no readable name. */
  const removingName = $derived(removing ? (removing.rec?.name ?? removing.server?.name ?? '') : '');
  const typedOk = $derived(typed.trim().toLowerCase() === (removingName || 'delete').trim().toLowerCase());

  function remove(rec: ShelfRecord | null, server: OtherCave | null = onServer(rec)) {
    typed = '';
    removeProblem = '';
    removing = { rec, server, step: 'choose' };
  }
  async function removeHere() {
    const rec = removing?.rec;
    if (!rec) return;
    await forgetCave(rec.id);
    if (picked === rec.id) picked = '';
    removing = null;
  }
  async function deleteIt() {
    const c = removing?.server;
    if (!c || !typedOk) return;
    removeBusy = true;
    removeProblem = await deleteEverywhere(c.id);
    removeBusy = false;
    if (removeProblem) return;
    if (removing?.rec && picked === removing.rec.id) picked = '';
    if (picked === `server:${c.id}`) picked = '';
    removing = null;
  }
  async function leaveIt() {
    const c = removing?.server;
    if (!c) return;
    removeBusy = true;
    removeProblem = await leaveShared(c.id);
    removeBusy = false;
    if (removeProblem) return;
    if (picked === `server:${c.id}` || (removing?.rec && picked === removing.rec.id)) picked = '';
    removing = null;
  }

  /** The right-click menu on a kept cave: the same things as the buttons beside it. */
  function rightClick(e: MouseEvent, id: string) {
    e.preventDefault();
    picked = id;
    const rec = shelf.records.find((r) => r.id === id);
    if (!rec) return;
    openContextMenu(e.clientX, e.clientY, [
      { heading: rec.name },
      { label: 'Open', run: () => void open() },
      { label: from ? `New trip from ${from}` : 'New trip', run: () => void newTrip() },
      'separator',
      { label: rec.favorite ? 'Remove from favourites' : 'Add to favourites ★', run: () => void setFavorite(id, !rec.favorite) },
      { label: 'Download a copy…', run: () => void download(id) },
      ...(rec.home?.path && isDesktop() ? [{ label: 'Show its folder', run: () => void reveal(rec) }] : []),
      ...(rec.home ? [{ label: 'Find its folder…', run: () => void find(rec) }] : []),
      'separator',
      { label: 'Remove…', danger: true, run: () => remove(rec) },
    ]);
  }

  function key(e: KeyboardEvent) {
    if (e.key === 'Escape') removing ? (removing = null) : (shelf.open = false);
    else if (e.key === 'Enter' && !(e.target as HTMLElement).closest('input,button,textarea')) void open();
  }

  const close = () => (shelf.open = false);
</script>

{#if shelf.open}
  <div class="veil" role="presentation" onclick={close}>
    <div class="box" role="dialog" aria-modal="true" aria-label="Caves" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={key}>
      {#if removing}
        {@const c = removing.server}
        {@const name = removingName || 'this cave'}
        <div class="remove-veil" role="presentation" onclick={() => (removing = null)}></div>
        <div class="remove" role="alertdialog" aria-modal="true" aria-label="Remove {name}">
          {#if removing.step === 'choose'}
            <h3>Remove “{name}”</h3>
            {#if removing.rec}
              <div class="choice">
                <button onclick={() => void removeHere()}>Remove from this computer</button>
                <p>{c ? 'It stays on the CaveCAD server and your other devices, and shows here again under “On your other devices”.' : 'The copy this browser keeps is deleted. A file you saved is not touched.'}</p>
              </div>
            {/if}
            {#if c?.role === 'owner'}
              <div class="choice">
                <button class="danger" disabled={c.id === engine.roomId()} onclick={() => ((removing!.step = 'delete'), (typed = ''))}>Delete everywhere…</button>
                <p>{c.id === engine.roomId() ? 'It is open now: open another cave first, then delete this one.' : 'Deletes it for good, from the server, all your devices and anyone you shared it with.'}</p>
              </div>
            {:else if c}
              <div class="choice">
                <button class="danger" onclick={() => void leaveIt()} disabled={removeBusy}>Leave this cave</button>
                <p>{c.ownerName || 'Its owner'} shared it with you. Leaving takes it off your devices; it carries on for them. They can share it with you again.</p>
              </div>
            {/if}
          {:else}
            <h3>Delete “{name}” everywhere?</h3>
            <p>This deletes the cave for good: from the CaveCAD server, from all your devices, and from everyone you shared it with. It cannot be undone. Files you saved or exported are not touched.</p>
            <label class="confirm">Type {removingName ? 'the cave’s name' : 'DELETE'} to confirm
              <!-- svelte-ignore a11y_autofocus: the one thing to do here is type -->
              <input bind:value={typed} placeholder={removingName || 'DELETE'} autofocus onkeydown={(e) => e.key === 'Enter' && typedOk && void deleteIt()} />
            </label>
          {/if}
          {#if removeProblem}<p class="err" role="alert">{removeProblem}</p>{/if}
          <div class="row">
            <button onclick={() => (removing = null)}>Cancel</button>
            {#if removing.step === 'delete'}<button class="danger solid" onclick={() => void deleteIt()} disabled={!typedOk || removeBusy}>{removeBusy ? 'Deleting…' : 'Delete for good'}</button>{/if}
          </div>
        </div>
      {/if}
      <div class="rail">
        <input type="search" placeholder="Search caves" bind:value={search} aria-label="Search caves" />
        <div class="list" role="listbox" aria-label="Caves">
          {#each shown as r (r.id)}
            <button class="cave" class:sel={picked === r.id} role="option" aria-selected={picked === r.id} onclick={() => (picked = r.id)} ondblclick={open} oncontextmenu={(e) => rightClick(e, r.id)}>
              <span class="name">{#if r.favorite}<Star size={12} fill="currentColor" />{/if} {r.name}</span>
              <span class="sub">{r.summary ? (r.summary.empty ? 'nothing surveyed yet' : `${r.summary.trips.length} trip${r.summary.trips.length === 1 ? '' : 's'} · ${length(r.summary.length_m)}`) : '…'}{r.id === shelf.currentId ? ' · open now' : ''}{sharedOf(r.id) ? ` · ${whose(sharedOf(r.id)!)}` : ''}</span>
            </button>
          {:else}
            <p class="none">{shelf.records.length ? 'No cave matches.' : 'No caves yet. Start one, or bring one in from a file.'}</p>
          {/each}
          {#if remoteShared.length}
            <div class="group" title="Caves of other members that you are a guest of">Shared with you</div>
            {#each remoteShared as c (c.id)}
              <button class="cave" class:sel={picked === `server:${c.id}`} role="option" aria-selected={picked === `server:${c.id}`} onclick={() => (picked = `server:${c.id}`)} ondblclick={open}>
                <span class="name">{c.name ?? 'A shared cave'}</span>
                <span class="sub">{serverCaves.opening === c.id ? 'fetching…' : whose(c)}</span>
              </button>
            {/each}
          {/if}
          {#if remoteMine.length}
            <div class="group" title="Caves you keep on the CaveCAD server that this computer does not have yet">On your other devices</div>
            {#each remoteMine as c (c.id)}
              <button class="cave" class:sel={picked === `server:${c.id}`} role="option" aria-selected={picked === `server:${c.id}`} onclick={() => (picked = `server:${c.id}`)} ondblclick={open}>
                <span class="name">{c.name ?? 'A cave from another device'}</span>
                <span class="sub">{serverCaves.opening === c.id ? 'fetching…' : !c.hasKey ? 'still coming over' : c.summary && !c.summary.empty ? `${c.summary.trips.length} trip${c.summary.trips.length === 1 ? '' : 's'} · ${length(c.summary.length_m)}` : 'on the server'}</span>
              </button>
            {/each}
          {/if}
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
          <button onclick={importFile} title="A cave saved as a DXF by CaveCAD, a survey from Compass, Walls, Survex, Therion or a CSV file, or a TopoDroid survey ZIP (shots and sketches)">Import file…</button>
        </div>
      </div>

      <div class="detail">
        {#if remote && !summary}
          <h2>{remote.name ?? 'A cave from another device'}</h2>
          <p class="stats">Kept on the CaveCAD server by one of your other devices · made {when(remote.createdAt)}</p>
          {#if !remote.hasKey}
            <p class="wait">{comingOver(remote)}</p>
          {:else}
            <p class="stored">Open it to bring it onto this computer. It stays in step with your other devices.</p>
          {/if}
        {:else if (record || sample || remote) && summary}
          <h2>{sample ? sample.title.replace(/ \(.*\)$/, '') : summary.name}</h2>
          <p class="stats">
            {#if summary.empty}Nothing surveyed yet.{:else}{length(summary.length_m)} surveyed · {summary.stations} stations · {summary.trips.length} trip{summary.trips.length === 1 ? '' : 's'}{#if summary.depth_m !== null && summary.depth_m > 0}{' · '}{length(summary.depth_m)} deep{/if} · <span class="health" title={HEALTH_HINT}>{health(summary)}</span>{/if}
          </p>
          {#if remote}
            <p class="stored">On the CaveCAD server, from one of your other devices · made {when(remote.createdAt)}. Open it to bring it onto this computer; it stays in step with your other devices.</p>
            {#if !remote.hasKey}<p class="wait">{comingOver(remote)}</p>{/if}
          {:else if record?.home && lost}
            <div class="lost" role="alert">
              <span>Folder not found: {record.home.label}</span>
              <button onclick={() => find(record!)}>Find folder…</button>
              <button onclick={() => keepHere(record!)}>Keep in this browser</button>
            </div>
          {:else if record?.home}<p class="stored" title={record.home.label}>In its folder, {record.home.label} · last saved {when(record.updated)}</p>
          {:else if record}<p class="stored">Kept in this browser · last saved {when(record.updated)}</p>{:else}<p class="stored">A sample cave. Open it to look around. Your first change makes it your own copy, kept in this browser; the sample stays as it is.</p>{/if}
          {#if record}
            <div class="actions">
              <button onclick={() => void setFavorite(record!.id, !record!.favorite)} title="Favourites stay at the top of the list"><Star size={13} fill={record.favorite ? 'currentColor' : 'none'} /> {record.favorite ? 'Favourite' : 'Add to favourites'}</button>
              <button onclick={() => void download(record!.id)} title="Save a copy of this cave as a file, to keep or to send to someone">Download a copy…</button>
              {#if record.home?.path && isDesktop()}<button onclick={() => void reveal(record!)}>Show its folder</button>{/if}
              {#if record.home && !lost}<button onclick={() => void find(record!)} title="Point CaveCAD at this cave's folder if you moved it">Find its folder…</button>{/if}
            </div>
          {/if}
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
        {#if remote && serverCaves.problem}<p class="err" role="alert">{serverCaves.problem}</p>{/if}
        {#if message}<p class="err" role="alert">{message}</p>{/if}
        <div class="spacer"></div>
        <div class="foot">
          <label><input type="checkbox" checked={shelf.startup} onchange={(e) => setStartup(e.currentTarget.checked)} /> Open this window at startup</label>
          {#if record}<button onclick={() => remove(record)} title="Take this cave off this computer, or delete it everywhere">Remove…</button>
          {:else if remote}<button onclick={() => remove(null, remote)} title={remote.role === 'owner' ? 'Delete this cave everywhere' : 'Leave this shared cave'}>{remote.role === 'owner' ? 'Delete…' : 'Leave…'}</button>{/if}
          <span class="grow"></span>
          <button onclick={newTrip} disabled={(!record && !sample) || !summary} title="Open the cave and start a new trip on it">{from ? `New trip from ${from}` : 'New trip'}</button>
          <button class="go" onclick={open} disabled={(!record && !sample && !remote) || !!serverCaves.opening}>{remote && serverCaves.opening === remote.id ? 'Fetching…' : remote && serverCaves.waitingFor === remote.id ? 'Waiting…' : 'Open'}</button>
          <button onclick={close}>{cave.hasCave ? 'Close' : 'Start empty'}</button>
        </div>
      </div>
    </div>
  </div>

{/if}

<style>
  .veil { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); display: grid; place-items: center; z-index: 40; }
  .box { width: min(980px, 96vw); height: min(600px, 92vh); background: var(--panel); color: var(--text); border: 1px solid var(--line); border-radius: 10px; display: flex; outline: none; overflow: hidden; }
  .rail { width: 290px; flex: none; border-right: 1px solid var(--line); display: flex; flex-direction: column; gap: 8px; padding: 12px; }
  .list { flex: 1; overflow: auto; display: flex; flex-direction: column; gap: 2px; }
  .group { font-size: 11px; font-weight: 600; color: var(--muted); padding: 10px 6px 2px; }
  .cave { text-align: left; display: flex; flex-direction: column; gap: 1px; padding: 6px 8px; border: none; background: none; border-radius: 6px; color: var(--text); cursor: pointer; font: inherit; }
  .cave:hover { background: var(--panel-2); }
  .cave.sel { background: var(--accent-bg); }
  .cave .name { font-weight: 600; font-size: 13px; display: flex; align-items: center; gap: 4px; }
  .cave .sub { font-size: 11px; color: var(--muted); }
  .row { display: flex; gap: 6px; }
  .row button { flex: 1; }
  .detail { flex: 1; min-width: 0; padding: 16px 20px; display: flex; flex-direction: column; gap: 8px; }
  h2 { margin: 0; font-size: 22px; }
  .stats { margin: 0; color: var(--text); font-size: 13px; }
  .health { text-decoration: underline dotted; cursor: help; }
  .lost { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; font-size: 12px; color: var(--danger); }
  .lost span { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .stored { margin: 0; color: var(--muted); font-size: 11px; }
  .actions { display: flex; flex-wrap: wrap; gap: 6px; }
  .actions button { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; padding: 3px 8px; }
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
  .wait { margin: 0; font-size: 12px; color: var(--text); background: var(--accent-bg); border: 1px solid var(--accent); border-radius: 6px; padding: 6px 10px; }
  .err { margin: 0; color: var(--danger); font-size: 12px; }
  input[type='search'] { font: inherit; font-size: 13px; padding: 6px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); }
  button { font: inherit; font-size: 13px; padding: 6px 12px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  button.go { background: var(--accent); border-color: var(--accent); color: #fff; }
  .box { position: relative; }
  .remove-veil { position: absolute; inset: 0; background: rgba(0, 0, 0, 0.45); z-index: 2; }
  .remove { position: absolute; z-index: 3; left: 50%; top: 50%; transform: translate(-50%, -50%); width: min(440px, 90%); background: var(--panel); border: 1px solid var(--line); border-radius: 10px; padding: 16px; display: flex; flex-direction: column; gap: 10px; box-shadow: 0 10px 30px rgba(0, 0, 0, 0.4); }
  .remove h3 { margin: 0; font-size: 15px; }
  .remove p { margin: 0; font-size: 12px; color: var(--muted); }
  .choice { display: flex; flex-direction: column; gap: 4px; align-items: flex-start; }
  .confirm { display: flex; flex-direction: column; gap: 4px; font-size: 12px; }
  .confirm input { font: inherit; padding: 6px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); }
  .remove .row { display: flex; justify-content: flex-end; gap: 8px; }
  button.danger { color: var(--danger); border-color: var(--danger); }
  button.danger.solid { background: var(--danger); color: #fff; }
</style>
