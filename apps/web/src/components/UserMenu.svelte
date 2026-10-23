<script lang="ts">
  // The user icon at the very top right: opens a window with who is here (this device and, in a shared
  // session, everyone else), your name and a few options, and a way to report a problem or send an idea
  // with the log and a screenshot attached.
  import { onMount } from 'svelte';
  import { Users, LogOut, Camera, Trash2, Copy, Download, Send, ImagePlus, Link, Lock } from '@lucide/svelte';
  import { cave, setCompactRibbon } from '../lib/cave.svelte';
  import { leaveP2p, p2p } from '../lib/p2p.svelte';
  import { shelf } from '../lib/shelf.svelte';
  import { activityOf, createInvite, inviteLink, join, leave, live, me, rename, revokeInvite, setLocked } from '../lib/sync.svelte';
  import { setUnits } from '../lib/cave.svelte';
  import { resetLayout } from '../lib/workspace.svelte';
  import { snapshotPage } from '../lib/snapshot';
  import { KIND_LABEL, buildReport, download, logText, send, type FeedbackKind } from '../lib/feedback';
  import { account, saveProfile } from '../lib/account.svelte';
  import AccountSection from './account/AccountSection.svelte';
  import DrivesSection from './account/DrivesSection.svelte';
  import AdminSection from './account/AdminSection.svelte';

  let open = $state(false);
  let tab = $state<'account' | 'drives' | 'people' | 'options' | 'feedback' | 'admin'>('account');
  const isAdmin = $derived(account.member?.role === 'admin');
  const SECTIONS = [['account', 'Account'], ['drives', 'Drives'], ['people', 'People'], ['options', 'Options'], ['feedback', 'Feedback']] as const;
  let root: HTMLDivElement;

  const initials = (name: string) => {
    const w = name.trim().split(/\s+/).filter(Boolean);
    return ((w[0]?.[0] ?? '?') + (w.length > 1 ? w[w.length - 1][0] : '')).toUpperCase();
  };
  const shared = $derived(live.status === 'live' || live.status === 'reconnecting');
  const others = $derived(shared ? Math.max(0, live.people.length - 1) : 0);
  const people = $derived([...live.people].sort((a, b) => (a.id === live.me ? -1 : b.id === live.me ? 1 : 0)));
  let room = $state('pitfall');
  // invites (the owner of a room makes them)
  let label = $state('');
  let hours = $state(24);
  let uses = $state(1);
  let copied = $state('');
  const made = $derived(live.invites.find((i) => i.token === live.made));
  async function copyLink(token: string) {
    await navigator.clipboard.writeText(inviteLink(token)).catch(() => {});
    copied = token;
    setTimeout(() => copied === token && (copied = ''), 2500);
  }
  const leftText = (i: { expires: number; uses: number; used: number }) => {
    const h = Math.max(0, Math.round((i.expires - Date.now()) / 3600_000));
    const when = h >= 48 ? `${Math.round(h / 24)} days left` : h >= 1 ? `${h} h left` : 'under an hour left';
    return `${i.uses === 0 ? 'anyone' : `${i.used}/${i.uses} used`} · ${when}`;
  };
  // a page opened from an invite link shows it straight away
  $effect(() => {
    if (live.pending) {
      open = true;
      tab = 'people';
      room = live.pending.room;
    }
  });
  const on = $derived(live.status !== 'off');

  // ---- feedback ----
  let kind = $state<FeedbackKind>('bug');
  let message = $state('');
  let includeLog = $state(true);
  let includeDetails = $state(true);
  let shot = $state<Blob | null>(null);
  let shotUrl = $state('');
  let note = $state('');
  let busy = $state(false);
  let hidden = $state(false); // the window steps aside while the screen is photographed

  function setShot(b: Blob | null) {
    if (shotUrl) URL.revokeObjectURL(shotUrl);
    shot = b;
    shotUrl = b ? URL.createObjectURL(b) : '';
  }
  async function capture() {
    note = '';
    hidden = true; // the window steps aside so it is not in the picture
    await new Promise((r) => setTimeout(r, 120));
    const b = await snapshotPage();
    hidden = false;
    if (b) setShot(b);
    else note = 'The screenshot did not work in this browser. Attach or paste an image instead.';
  }
  function attach(files: FileList | null | undefined) {
    const f = [...(files ?? [])].find((x) => x.type.startsWith('image/'));
    if (f) setShot(f);
  }
  function paste(e: ClipboardEvent) {
    if (tab === 'feedback') attach(e.clipboardData?.files);
  }
  const draft = () => ({ kind, message, includeLog, includeDetails, screenshot: shot });
  async function save() {
    busy = true;
    const r = await buildReport(draft());
    download(r.name, r.zip);
    note = `Saved ${r.name}`;
    busy = false;
  }
  async function copy() {
    const r = await buildReport({ ...draft(), screenshot: null });
    await navigator.clipboard.writeText(r.text + (includeLog ? `\n\n--- log ---\n${logText()}` : '')).catch(() => {});
    note = 'Copied the text (the screenshot is only in the saved report).';
  }
  async function sendIt() {
    busy = true;
    const r = await buildReport(draft());
    const out = await send(r.name, r.zip);
    note = out.message;
    if (out.ok) {
      message = '';
      setShot(null);
    }
    busy = false;
  }

  function outside(e: PointerEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
  onMount(() => () => shotUrl && URL.revokeObjectURL(shotUrl));
</script>

<svelte:window onpointerdown={outside} onkeydown={(e) => e.key === 'Escape' && open && (open = false)} onpaste={paste} />

<div class="user" bind:this={root} class:away={hidden}>
  <button class="icon" style="--c:{me.color}" aria-haspopup="dialog" aria-expanded={open} title={shared ? `${me.name} · ${live.people.length} here` : `${me.name}: options and feedback`} onclick={() => (open = !open)}>
    <span class="face">{initials(me.name)}</span>
    {#if others > 0}<span class="badge">{others}</span>{/if}
  </button>

  {#if open}
    <div class="win" role="dialog" aria-label="You, options and feedback">
      <div class="head">
        <span class="face big" style="--c:{me.color}">{initials(me.name)}</span>
        <div class="who">
          <input class="name" value={me.name} aria-label="Your name" maxlength="40" onchange={(e) => { rename(e.currentTarget.value); void saveProfile({ displayName: e.currentTarget.value }).catch(() => {}); }} />
          <span class="sub">{shared ? `Live in ${live.room}` : 'Working on your own'}</span>
        </div>
      </div>
      <div class="body">
      <div class="tabs" role="tablist" aria-orientation="vertical">
        {#each SECTIONS as [id, label]}
          <button role="tab" aria-selected={tab === id} class:on={tab === id} onclick={() => (tab = id)}>{label}</button>
        {/each}
        {#if isAdmin}
          <hr />
          <button role="tab" aria-selected={tab === 'admin'} class:on={tab === 'admin'} onclick={() => (tab = 'admin')}>Admin</button>
        {/if}
      </div>
      <div class="content">
      {#if tab === 'account'}
        <AccountSection />
      {:else if tab === 'drives'}
        <DrivesSection />
      {:else if tab === 'admin' && isAdmin}
        <AdminSection />
      {:else if tab === 'people'}
        <div class="pane">
          {#if p2p.role !== 'none'}
            <ul class="list" aria-label="People in this session">
              {#each people as p (p.id)}
                <li>
                  <span class="face" style="--c:{p.color}">{initials(p.name)}</span>
                  <span class="pn">{p.name}{p.id === live.me ? ' (you)' : ''}{p.id === live.ownerId ? ' · host' : ''}</span>
                  <span class="act">{p.id === live.me ? '' : (activityOf(p.id) ?? '')}</span>
                </li>
              {/each}
            </ul>
            <div class="status" role="status">{live.status === 'connecting' ? p2p.waiting || 'Connecting…' : `${live.people.length} ${live.people.length === 1 ? 'person' : 'people'} in this session`}</div>
            {#if p2p.role === 'host'}
              <button class="btn primary" onclick={() => { p2p.dialog = true; open = false; }}><Link size={15} /> Invite someone</button>
            {/if}
            <button class="btn" onclick={() => { leaveP2p(); leave(); }}><LogOut size={15} /> {p2p.role === 'host' ? 'Stop sharing' : 'Leave this session'}</button>
          {:else if !shared && live.status === 'off' && !live.pending}
            <p class="hint">Work on this cave together, live. Devices connect straight to each other: there is no server to set up.</p>
            <div class="row">
              <button class="btn primary" disabled={!cave.hasCave} title={cave.hasCave ? '' : 'Open a cave first'} onclick={() => { p2p.dialog = true; open = false; }}><Users size={15} /> Share this cave live</button>
              <button class="btn" onclick={() => { shelf.join = true; shelf.open = true; open = false; }}>Join a live session</button>
            </div>
          {:else if shared || live.status === 'connecting'}
            <ul class="list" aria-label="People in this session">
              {#each people as p (p.id)}
                <li>
                  <span class="face" style="--c:{p.color}">{initials(p.name)}</span>
                  <span class="pn">{p.name}{p.id === live.me ? ' (you)' : ''}{p.id === live.ownerId ? ' · owner' : ''}</span>
                  <span class="act">{p.id === live.me ? '' : (activityOf(p.id) ?? '')}</span>
                </li>
              {/each}
            </ul>
            <div class="status" role="status">{live.status === 'reconnecting' ? 'Reconnecting… your edits are kept' : live.status === 'connecting' ? 'Connecting…' : `${live.people.length} ${live.people.length === 1 ? 'person' : 'people'} in ${live.room}`}</div>
            {#if live.owner}
              <div class="invites">
                <div class="sec"><Link size={14} /> Invite people</div>
                <div class="row"><input class="room" bind:value={label} maxlength="60" aria-label="Who is the invite for" placeholder="Who is it for? (optional)" /></div>
                <div class="row">
                  <select bind:value={hours} aria-label="How long it works">
                    <option value={1}>For 1 hour</option><option value={24}>For 1 day</option><option value={168}>For 7 days</option>
                  </select>
                  <select bind:value={uses} aria-label="Who can use it">
                    <option value={1}>One person</option><option value={5}>Up to 5 people</option><option value={0}>Anyone with the link</option>
                  </select>
                  <button class="btn primary" onclick={() => createInvite(label, hours, uses)}>Make invite</button>
                </div>
                {#if made}
                  <div class="link" role="status">
                    <input readonly value={inviteLink(made.token)} aria-label="The invite link" onfocus={(e) => e.currentTarget.select()} />
                    <button class="btn" onclick={() => copyLink(made.token)}><Copy size={14} /> {copied === made.token ? 'Copied' : 'Copy'}</button>
                  </div>
                {/if}
                {#if live.invites.length}
                  <ul class="list" aria-label="Invites you made">
                    {#each live.invites as i (i.token)}
                      <li>
                        <span class="pn">{i.label || 'Invite'}<span class="act"> · {leftText(i)}</span></span>
                        <button class="btn small" title="Copy this invite's link" onclick={() => copyLink(i.token)}>{copied === i.token ? 'Copied' : 'Copy link'}</button>
                        <button class="btn small" title="Stop this invite working" onclick={() => revokeInvite(i.token)}><Trash2 size={13} /></button>
                      </li>
                    {/each}
                  </ul>
                {/if}
                <label class="check"><input type="checkbox" checked={live.locked} onchange={(e) => setLocked(e.currentTarget.checked)} /> <span><Lock size={12} /> Only people with an invite can join</span></label>
              </div>
            {/if}
            <button class="btn" onclick={leave}><LogOut size={15} /> Leave this session</button>
          {:else}
            {#if live.pending}
              <p class="hint">You were invited to <strong>{live.pending.room}</strong>. Check your name above, then join.</p>
            {:else}
              <p class="hint">Share this cave live: start a room, then invite people from here.</p>
            {/if}
            <div class="row">
              <input class="room" bind:value={room} aria-label="Room name" placeholder="Room name" onkeydown={(e) => e.key === 'Enter' && !on && join(room, live.pending?.room === room ? live.pending.invite : '')} />
              <button class="btn primary" onclick={() => join(room, live.pending?.room === room ? live.pending.invite : '')}><Users size={15} /> {live.pending ? 'Join the room' : 'Start or join'}</button>
            </div>
            {#if live.error}<div class="status bad" role="alert">{live.error}</div>{/if}
          {/if}
        </div>
      {:else if tab === 'options'}
        <div class="pane">
          <div class="row"><span class="lab">Lengths</span>
            <div class="seg" role="group" aria-label="Units">
              <button class:on={cave.units === 'metric'} onclick={() => setUnits('metric')}>Metric</button>
              <button class:on={cave.units === 'imperial'} onclick={() => setUnits('imperial')}>Imperial</button>
            </div>
          </div>
          <label class="check"><input type="checkbox" checked={cave.compactRibbon} onchange={(e) => setCompactRibbon(e.currentTarget.checked)} /> Compact ribbon (every group folds into a dropdown)</label>
          <div class="row"><button class="btn" onclick={resetLayout}>Reset the panel layout</button></div>
          <p class="hint">Theme and colours are on the View tab.</p>
        </div>
      {:else}
        <div class="pane">
          <div class="row">
            <select bind:value={kind} aria-label="What kind of feedback">
              {#each Object.entries(KIND_LABEL) as [k, label]}<option value={k}>{label}</option>{/each}
            </select>
          </div>
          <textarea bind:value={message} rows="4" maxlength="5000" aria-label="Your message" placeholder={kind === 'bug' ? 'What happened, and what did you expect?' : kind === 'idea' ? 'What would make this better?' : 'What would you like to know?'}></textarea>
          <label class="check"><input type="checkbox" bind:checked={includeLog} /> Include the log (what the app has been doing; no positions)</label>
          <label class="check"><input type="checkbox" bind:checked={includeDetails} /> Include app details (browser, screen size, counts; not your cave)</label>
          <div class="shot">
            {#if shotUrl}
              <img src={shotUrl} alt="The screenshot that will be sent" />
              <button class="btn small" onclick={() => setShot(null)}><Trash2 size={14} /> Remove</button>
            {:else}
              <button class="btn" onclick={capture}><Camera size={15} /> Take a screenshot</button>
              <label class="btn"><ImagePlus size={15} /> Attach an image<input type="file" accept="image/*" hidden onchange={(e) => attach(e.currentTarget.files)} /></label>
              <span class="hint">or paste one</span>
            {/if}
          </div>
          <div class="row actions">
            <button class="btn primary" disabled={busy || (!message.trim() && !shot)} onclick={sendIt}><Send size={15} /> Send</button>
            <button class="btn" disabled={busy} onclick={save}><Download size={15} /> Save report</button>
            <button class="btn" disabled={busy} onclick={copy}><Copy size={15} /> Copy text</button>
          </div>
          {#if note}<div class="status" role="status">{note}</div>{/if}
        </div>
      {/if}
      </div>
      </div>
    </div>
  {/if}
</div>

<style>
  .user { margin-left: auto; align-self: center; position: relative; padding-right: 2px; }
  .user.away { visibility: hidden; }
  .icon { position: relative; display: grid; place-items: center; width: 30px; height: 30px; padding: 0; border-radius: 50%; border: 2px solid transparent; background: none; cursor: pointer; }
  .icon:hover, .icon[aria-expanded='true'] { border-color: var(--accent); }
  .face { width: 26px; height: 26px; box-sizing: border-box; display: grid; place-items: center; border-radius: 50%; background: var(--c); color: #fff; font-size: 11px; font-weight: 600; user-select: none; flex: none; }
  .face.big { width: 40px; height: 40px; font-size: 15px; }
  .badge { position: absolute; right: -4px; bottom: -4px; min-width: 15px; height: 15px; padding: 0 3px; box-sizing: border-box; border-radius: 8px; background: var(--accent); color: #fff; font-size: 10px; line-height: 15px; text-align: center; }
  .win { position: fixed; top: 44px; right: 8px; z-index: 60; width: 520px; max-width: calc(100vw - 16px); max-height: calc(100vh - 56px); overflow: auto; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); box-shadow: 0 8px 28px rgb(0 0 0 / 0.35); color: var(--text); font-size: 13px; }
  .head { display: flex; align-items: center; gap: 12px; padding: 14px 14px 10px; }
  .who { display: flex; flex-direction: column; gap: 2px; min-width: 0; flex: 1; }
  .name { font: inherit; font-size: 15px; font-weight: 600; color: var(--text-strong); background: none; border: 1px solid transparent; border-radius: 6px; padding: 2px 6px; margin-left: -6px; }
  .name:hover, .name:focus { border-color: var(--line); background: var(--panel-2); outline: none; }
  .sub { color: var(--muted); font-size: 12px; }
  .body { display: grid; grid-template-columns: 120px minmax(0, 1fr); border-top: 1px solid var(--line); }
  .tabs { display: flex; flex-direction: column; gap: 2px; padding: 8px 6px; border-right: 1px solid var(--line); }
  .tabs hr { border: 0; border-top: 1px solid var(--line); margin: 6px 0; width: 100%; }
  .tabs button { background: none; border: 0; padding: 6px 10px; color: var(--muted); cursor: pointer; border-radius: var(--radius); text-align: left; font: inherit; }
  .tabs button.on { color: var(--text-strong); background: var(--accent-bg); }
  .content { min-width: 0; }
  @media (max-width: 560px) {
    .body { grid-template-columns: minmax(0, 1fr); }
    .tabs { flex-direction: row; flex-wrap: wrap; border-right: 0; border-bottom: 1px solid var(--line); }
    .tabs hr { display: none; }
  }
  .pane { display: flex; flex-direction: column; gap: 10px; padding: 12px 14px 14px; }
  .list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
  .list li { display: flex; align-items: center; gap: 10px; }
  .pn { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .act { color: var(--muted); font-size: 12px; }
  .row { display: flex; align-items: center; gap: 8px; }
  .lab { min-width: 60px; color: var(--muted); }
  .hint { margin: 0; color: var(--muted); font-size: 12px; }
  .status { font-size: 12px; color: var(--muted); }
  .status.bad { color: var(--danger); }
  .check { display: flex; align-items: flex-start; gap: 8px; font-size: 12px; }
  .room, select, textarea { font: inherit; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); padding: 5px 8px; box-sizing: border-box; }
  .room { flex: 1; min-width: 0; }
  textarea { width: 100%; resize: vertical; }
  .btn { display: inline-flex; align-items: center; gap: 6px; padding: 5px 10px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); cursor: pointer; font: inherit; font-size: 12px; }
  .btn:hover:not(:disabled) { background: var(--accent-bg); }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .btn.primary { background: var(--accent); color: #fff; border-color: var(--accent); }
  .btn.small { padding: 3px 8px; }
  .seg { display: flex; }
  .seg button { padding: 4px 12px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); cursor: pointer; font: inherit; font-size: 12px; }
  .seg button:first-child { border-radius: var(--radius) 0 0 var(--radius); }
  .seg button:last-child { border-radius: 0 var(--radius) var(--radius) 0; border-left: 0; }
  .seg button.on { background: var(--accent-bg); outline: 1px solid var(--accent); position: relative; }
  .shot { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
  .shot img { max-width: 100%; max-height: 130px; border: 1px solid var(--line); border-radius: 6px; }
  .actions { flex-wrap: wrap; }
  .invites { display: flex; flex-direction: column; gap: 8px; padding-top: 10px; border-top: 1px solid var(--line); }
  .sec { display: flex; align-items: center; gap: 6px; font-weight: 600; color: var(--text-strong); }
  .link { display: flex; gap: 6px; }
  .link input { flex: 1; min-width: 0; font: inherit; font-size: 11px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); padding: 4px 6px; }
</style>
