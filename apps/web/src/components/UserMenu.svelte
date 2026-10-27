<script lang="ts">
  // The user icon at the very top right: opens a window with your account, your name and a few options, and a
  // way to report a problem or send an idea with the log and a screenshot attached.
  import { onMount } from 'svelte';
  import { Camera, Trash2, Copy, Download, Send, ImagePlus } from '@lucide/svelte';
  import { cave, setCompactRibbon } from '../lib/cave.svelte';
  import { me, rename } from '../lib/me.svelte';
  import { setUnits } from '../lib/cave.svelte';
  import { resetLayout } from '../lib/workspace.svelte';
  import { snapshotPage } from '../lib/snapshot';
  import { KIND_LABEL, buildReport, download, logText, send, type FeedbackKind } from '../lib/feedback';
  import { account, saveProfile } from '../lib/account.svelte';
  import AccountSection from './account/AccountSection.svelte';
  import AdminSection from './account/AdminSection.svelte';

  let open = $state(false);
  let tab = $state<'account' | 'options' | 'feedback' | 'admin'>('account');
  const isAdmin = $derived(account.member?.role === 'admin');
  // Drives is left out of this release: connecting cloud drives comes later (DrivesSection.svelte is kept for it).
  const SECTIONS = [['account', 'Account'], ['options', 'Options'], ['feedback', 'Feedback']] as const;
  let root: HTMLDivElement;

  const initials = (name: string) => {
    const w = name.trim().split(/\s+/).filter(Boolean);
    return ((w[0]?.[0] ?? '?') + (w.length > 1 ? w[w.length - 1][0] : '')).toUpperCase();
  };

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
  onMount(() => {
    const show = (e: Event) => { open = true; tab = (e as CustomEvent).detail ?? 'account'; };
    addEventListener('cavecad:open-user', show);
    return () => {
      removeEventListener('cavecad:open-user', show);
      if (shotUrl) URL.revokeObjectURL(shotUrl);
    };
  });
</script>

<svelte:window onpointerdown={outside} onkeydown={(e) => e.key === 'Escape' && open && (open = false)} onpaste={paste} />

<div class="user" bind:this={root} class:away={hidden}>
  <button class="icon" style="--c:{me.color}" aria-haspopup="dialog" aria-expanded={open} title={`${me.name}: options and feedback`} onclick={() => (open = !open)}>
    <span class="face">{initials(me.name)}</span>
  </button>

  {#if open}
    <div class="win" role="dialog" aria-label="You, options and feedback">
      <div class="head">
        <span class="face big" style="--c:{me.color}">{initials(me.name)}</span>
        <div class="who">
          <input class="name" value={me.name} aria-label="Your name" maxlength="40" onchange={(e) => { rename(e.currentTarget.value); void saveProfile({ displayName: e.currentTarget.value }).catch(() => {}); }} />
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
      {:else if tab === 'admin' && isAdmin}
        <AdminSection />
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
  .win { position: fixed; top: 44px; right: 8px; z-index: 60; width: 520px; max-width: calc(100vw - 16px); max-height: calc(100vh - 56px); overflow: auto; background: var(--panel); border: 1px solid var(--line); border-radius: var(--radius); box-shadow: 0 8px 28px rgb(0 0 0 / 0.35); color: var(--text); font-size: 13px; }
  .head { display: flex; align-items: center; gap: 12px; padding: 14px 14px 10px; }
  .who { display: flex; flex-direction: column; gap: 2px; min-width: 0; flex: 1; }
  .name { font: inherit; font-size: 15px; font-weight: 600; color: var(--text-strong); background: none; border: 1px solid transparent; border-radius: 6px; padding: 2px 6px; margin-left: -6px; }
  .name:hover, .name:focus { border-color: var(--line); background: var(--panel-2); outline: none; }
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
  .row { display: flex; align-items: center; gap: 8px; }
  .lab { min-width: 60px; color: var(--muted); }
  .hint { margin: 0; color: var(--muted); font-size: 12px; }
  .status { font-size: 12px; color: var(--muted); }
  .check { display: flex; align-items: flex-start; gap: 8px; font-size: 12px; }
  select, textarea { font: inherit; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); padding: 5px 8px; box-sizing: border-box; }
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
</style>
