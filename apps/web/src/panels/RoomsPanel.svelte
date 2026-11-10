<script lang="ts">
  // The Rooms panel: who the open cave is shared with. Left, the cave's rooms (Private first: owners only); right,
  // the picked room's guest list. Owners add, rename and remove rooms, add members as Editor or Watcher, change a
  // guest's seat and take guests off. Guests see the rooms they are in, read-only. The server enforces the rules
  // (lib/cave-rooms.svelte.ts says what it refused). People are managed, never their computers.
  import { Lock, Users, Plus, Trash2, Pencil, X, UserPlus, RefreshCw } from '@lucide/svelte';
  import { account } from '../lib/account.svelte';
  import { cave } from '../lib/cave.svelte';
  import { engine, loadEngine } from '../lib/engine';
  import { ROOM_TEXT, room } from '../lib/room.svelte';
  import { addByEmail, addRoom, cancelInvited, caveRooms, matches, readRooms, removeGuest, removeRoom, renameRoom, setGuest, type Member, type Seat } from '../lib/cave-rooms.svelte';

  const signedIn = $derived((account.phase === 'ready' || account.phase === 'offline') && !!account.member);

  // The open cave's id on the server: read again when a cave opens, the account changes, or sync reaches a new state
  // (a new cave gets its id when its room is made).
  let caveId = $state('');
  $effect(() => {
    void cave.opened;
    void cave.hasCave;
    void room.status;
    void account.member?.id;
    void loadEngine().then(() => (caveId = cave.hasCave && signedIn ? engine.roomId() : ''));
  });
  // Read now, and every 20 s while the panel shows (someone else may change the lists).
  $effect(() => {
    const id = caveId;
    void readRooms(id);
    const t = setInterval(() => void readRooms(id), 20_000);
    return () => clearInterval(t);
  });

  const owner = $derived(caveRooms.you === 'owner');
  let picked = $state('');
  const selected = $derived(caveRooms.rooms.find((r) => r.id === picked) ?? caveRooms.rooms.find((r) => r.kind === 'shared') ?? caveRooms.rooms[0] ?? null);

  let adding = $state(false);
  let newName = $state('');
  async function add() {
    const id = await addRoom(newName);
    if (id) {
      picked = id;
      adding = false;
      newName = '';
    }
  }

  let renaming = $state(false);
  let rename = $state('');
  async function doRename() {
    if (selected && (await renameRoom(selected.id, rename))) renaming = false;
  }
  async function remove() {
    if (!selected) return;
    const n = selected.guests.length;
    const who = n === 0 ? '' : ` Its ${n} guest${n === 1 ? '' : 's'} stop getting new edits unless they are in another room of this cave.`;
    if (window.confirm(`Remove the “${selected.name}” room?${who}`)) await removeRoom(selected.id);
  }

  let typed = $state('');
  let seat = $state<Seat>('editor');
  let chosen = $state<Member | null>(null);
  const found = $derived(chosen ? [] : matches(typed, selected));
  function choose(m: Member) {
    chosen = m;
    typed = `${m.name} (${m.email})`;
  }
  async function addGuest() {
    if (!selected) return;
    const m = chosen ?? (matches(typed, selected).length === 1 ? matches(typed, selected)[0] : null);
    // Not a member picked from the list: an email is added (or invited, by an admin) on the server.
    const ok = m ? await setGuest(selected.id, m.id, seat) : /@/.test(typed) ? await addByEmail(selected.id, typed.trim(), seat) : ((caveRooms.problem = "Pick a member from the list, or type someone's email to invite them."), false);
    if (ok) {
      typed = '';
      chosen = null;
    }
  }

  const roleText: Record<string, string> = { owner: 'you own this cave', editor: "you're an editor", watcher: "you're watching" };
  const syncText = $derived(room.info || (room.status ? ROOM_TEXT[room.status] + (room.status === 'error' && room.reason ? `: ${room.reason}` : '') : ''));
</script>

<div class="panel">
  <div class="bar">
    {#if cave.hasCave}<b>{cave.name}</b>{/if}
    {#if caveRooms.you}<span class="muted">· {roleText[caveRooms.you]}</span>{/if}
    <span class="sync" class:err={room.status === 'error'}>{syncText}</span>
    <button class="icon" onclick={() => void readRooms(caveId)} disabled={!caveId} title="Read the rooms from the server again" aria-label="Refresh"><RefreshCw size={14} /></button>
  </div>

  {#if !cave.hasCave}
    <p class="note">Open a cave to see its rooms.</p>
  {:else if !signedIn}
    <p class="note">Sign in (top right) to share this cave.</p>
  {:else if !caveId && !room.status}
    <p class="note">This cave isn't kept on the server, so it can't be shared. Samples never are: make a new cave, or open one of your own.</p>
  {:else if !caveRooms.loaded}
    <p class="note">{caveRooms.problem || 'Reading…'}</p>
  {:else}
    <div class="body">
      <div class="rooms" role="listbox" aria-label="Rooms">
        <div class="head">Rooms</div>
        {#each caveRooms.rooms as r (r.id)}
          <button class="room" class:sel={selected?.id === r.id} role="option" aria-selected={selected?.id === r.id} onclick={() => ((picked = r.id), (renaming = false), (typed = ''), (chosen = null))}>
            {#if r.kind === 'private'}<Lock size={13} />{:else}<Users size={13} />{/if}
            <span class="rname">{r.name}</span>
            <span class="count">{r.kind === 'private' ? 'owners' : r.guests.length}</span>
          </button>
        {/each}
        {#if owner}
          {#if adding}
            <form class="addroom" onsubmit={(e) => (e.preventDefault(), void add())}>
              <!-- svelte-ignore a11y_autofocus -->
              <input bind:value={newName} placeholder="Survey team" maxlength="60" aria-label="New room name" autofocus onkeydown={(e) => e.key === 'Escape' && (adding = false)} />
              <div class="row"><button type="submit" disabled={caveRooms.busy || !newName.trim()}>Add</button><button type="button" onclick={() => (adding = false)}>Cancel</button></div>
            </form>
          {:else}
            <button class="addbtn" onclick={() => ((adding = true), (newName = ''))}><Plus size={13} /> Add a room</button>
          {/if}
        {/if}
      </div>

      <div class="detail">
        {#if selected}
          <div class="title">
            {#if renaming}
              <form class="rename" onsubmit={(e) => (e.preventDefault(), void doRename())}>
                <!-- svelte-ignore a11y_autofocus -->
                <input bind:value={rename} maxlength="60" aria-label="Room name" autofocus onkeydown={(e) => e.key === 'Escape' && (renaming = false)} />
                <button type="submit" disabled={caveRooms.busy || !rename.trim()}>Save</button>
                <button type="button" onclick={() => (renaming = false)}>Cancel</button>
              </form>
            {:else}
              <h3>{selected.name}</h3>
              {#if owner && selected.kind === 'shared'}
                <button class="icon" onclick={() => ((renaming = true), (rename = selected.name))} title="Rename this room" aria-label="Rename this room"><Pencil size={13} /></button>
                <span class="grow"></span>
                <button class="danger" onclick={() => void remove()} disabled={caveRooms.busy}><Trash2 size={13} /> Remove room</button>
              {/if}
            {/if}
          </div>

          {#if selected.kind === 'private'}
            <p class="muted">Only this cave's owners are in the Private room.</p>
            <ul class="people">
              {#each caveRooms.owners as o (o.memberId)}
                <li><span class="who">{o.name} <span class="muted">{o.email}</span></span><span class="seat">Owner</span></li>
              {/each}
            </ul>
            {#if owner}<p class="muted">To share this cave, add a room and put people on its guest list.</p>{/if}
          {:else}
            <div class="head">Guests</div>
            <ul class="people">
              {#each selected.guests as g (g.memberId)}
                <li>
                  <span class="who">{g.name} <span class="muted">{g.email}</span></span>
                  {#if owner}
                    <select value={g.seat} aria-label="Seat for {g.name}" disabled={caveRooms.busy} onchange={(e) => void setGuest(selected.id, g.memberId, (e.currentTarget as HTMLSelectElement).value as Seat)}>
                      <option value="editor">Editor</option>
                      <option value="watcher">Watcher</option>
                    </select>
                    <button class="icon" onclick={() => void removeGuest(selected.id, g.memberId)} disabled={caveRooms.busy} title="Take {g.name} off this room" aria-label="Take {g.name} off this room"><X size={14} /></button>
                  {:else}
                    <span class="seat">{g.seat === 'editor' ? 'Editor' : 'Watcher'}</span>
                  {/if}
                </li>
              {:else}
                {#if !selected.invited.length}<li class="muted">No guests yet.</li>{/if}
              {/each}
              {#each selected.invited as p (p.email)}
                <li>
                  <span class="who">{p.email} <span class="muted">invited, not joined yet</span></span>
                  <span class="seat">{p.seat === 'editor' ? 'Editor' : 'Watcher'}</span>
                  <button onclick={() => void cancelInvited(selected.id, p.email)} disabled={caveRooms.busy} title="Cancel the seat kept for {p.email}">Cancel</button>
                </li>
              {/each}
            </ul>

            {#if owner}
              <form class="addguest" onsubmit={(e) => (e.preventDefault(), void addGuest())}>
                <div class="pick">
                  <input bind:value={typed} oninput={() => (chosen = null)} placeholder="A member's name, or anyone's email" aria-label="Member to add" autocomplete="off" />
                  {#if found.length}
                    <div class="drop" role="listbox" aria-label="Matching members">
                      {#each found as m (m.id)}
                        <button type="button" role="option" aria-selected="false" onclick={() => choose(m)}>{m.name} <span class="muted">{m.email}</span></button>
                      {/each}
                    </div>
                  {/if}
                </div>
                <select bind:value={seat} aria-label="Seat for the new guest">
                  <option value="editor">Editor</option>
                  <option value="watcher">Watcher</option>
                </select>
                <button type="submit" disabled={caveRooms.busy || !typed.trim()}><UserPlus size={13} /> Add</button>
              </form>
              {#if caveRooms.sent}
                <p class="sentnote">
                  {caveRooms.sent.emailed ? `Invite emailed to ${caveRooms.sent.email}.` : `Couldn't email ${caveRooms.sent.email}: send them this link yourself.`}
                  {#if caveRooms.sent.link && !caveRooms.sent.emailed}<button onclick={() => void navigator.clipboard?.writeText(caveRooms.sent!.link)}>Copy link</button>{/if}
                  They get this seat when they join.
                </p>
              {/if}
              <p class="muted">Editors can change the cave; watchers can only look. Type someone's email to invite them if they aren't a member yet (admins only). Someone taken off keeps the copy already on their computer but gets no new edits.</p>
            {/if}
          {/if}
        {/if}
        {#if caveRooms.problem}<p class="err" role="alert">{caveRooms.problem}</p>{/if}
      </div>
    </div>
  {/if}
</div>

<style>
  .panel { display: flex; flex-direction: column; height: 100%; background: var(--panel); color: var(--text); font-size: 13px; }
  .bar { display: flex; align-items: center; gap: 6px; padding: 6px 10px; border-bottom: 1px solid var(--line); min-height: 32px; }
  .sync { margin-left: auto; color: var(--muted); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .sync.err { color: var(--danger); white-space: normal; }
  button { display: inline-flex; align-items: center; gap: 5px; font: inherit; font-size: 12px; padding: 4px 10px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); cursor: pointer; }
  button.icon { padding: 4px 6px; background: transparent; border-color: transparent; }
  button.icon:hover:not(:disabled) { background: var(--hover); }
  button.danger { color: var(--danger); }
  button:disabled { opacity: 0.5; cursor: default; }
  input, select { font: inherit; font-size: 12px; padding: 4px 6px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--field); color: var(--text); }
  .note { padding: 12px; color: var(--muted); margin: 0; }
  .body { flex: 1; display: grid; grid-template-columns: minmax(130px, 34%) minmax(0, 1fr); min-height: 0; }
  .rooms { border-right: 1px solid var(--line); padding: 8px; overflow: auto; display: flex; flex-direction: column; gap: 2px; }
  .head { color: var(--muted); font-size: 12px; margin: 0 0 4px 2px; }
  .room { width: 100%; border: none; background: transparent; justify-content: flex-start; padding: 6px 8px; }
  .room:hover { background: var(--hover); }
  .room.sel { background: var(--accent-bg); color: var(--accent); }
  .rname { flex: 1; text-align: left; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .count { color: var(--muted); font-size: 11px; }
  .room.sel .count { color: var(--accent); }
  .addbtn { margin-top: 6px; justify-content: center; }
  .addroom { display: flex; flex-direction: column; gap: 4px; margin-top: 6px; }
  .row { display: flex; gap: 4px; }
  .detail { padding: 8px 12px; overflow: auto; min-width: 0; }
  .title { display: flex; align-items: center; gap: 4px; margin-bottom: 8px; }
  .grow { flex: 1; }
  h3 { margin: 0; font-size: 15px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .rename { display: flex; gap: 4px; flex: 1; }
  .rename input { flex: 1; min-width: 0; }
  .people { list-style: none; margin: 0 0 10px; padding: 0; }
  .people li { display: flex; align-items: center; gap: 6px; padding: 5px 0; border-bottom: 1px solid var(--line); }
  .who { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .seat { color: var(--muted); font-size: 12px; }
  .addguest { display: flex; gap: 6px; align-items: flex-start; }
  .pick { flex: 1; position: relative; min-width: 0; }
  .pick input { width: 100%; box-sizing: border-box; }
  .drop { position: absolute; left: 0; right: 0; top: 100%; z-index: 5; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); box-shadow: 0 4px 14px rgb(0 0 0 / 12%); display: flex; flex-direction: column; margin-top: 2px; }
  .drop button { border: none; border-radius: 0; background: transparent; justify-content: flex-start; padding: 6px 8px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .drop button:hover { background: var(--hover); }
  .muted { color: var(--muted); font-size: 12px; }
  p.muted { margin: 6px 0; }
  .sentnote { font-size: 12px; margin: 6px 0; display: flex; flex-wrap: wrap; gap: 6px; align-items: center; }
  .err { color: var(--danger); font-size: 12px; margin: 8px 0 0; }
</style>
