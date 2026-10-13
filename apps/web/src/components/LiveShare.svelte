<script lang="ts">
  import RelaySettings from './RelaySettings.svelte';
  import MailboxSettings from './MailboxSettings.svelte';
  // Sharing the open cave live, straight to other devices (no server). The host makes an invite code per
  // person and sends it by any message; the guest sends a reply code back; the host pastes it here. After
  // that everyone sees each other's edits, pointers and (if they follow) where the host is looking.
  import { Copy, X } from '@lucide/svelte';
  import { cave } from '../lib/cave.svelte';
  import { leaveP2p, letIn, makeInvite, p2p, remove, startHosting, startLinkSharing, takeReply, turnAway } from '../lib/p2p.svelte';
  import { live, me, rename } from '../lib/sync.svelte';

  let replies = $state<Record<string, string>>({});
  let problems = $state<Record<string, string>>({});
  let copied = $state('');

  async function connect(id: string) {
    const r = await takeReply(id, replies[id] ?? '');
    problems[id] = r ?? '';
    if (!r) replies[id] = '';
  }

  async function copy(id: string, text: string) {
    try {
      await navigator.clipboard.writeText(text);
      copied = id;
      setTimeout(() => copied === id && (copied = ''), 1500);
    } catch {
      /* the code is selectable: copy it by hand */
    }
  }

  const close = () => (p2p.dialog = false);
  let linkCopied = $state(false);
  async function copyLink() {
    try {
      await navigator.clipboard.writeText(p2p.link);
      linkCopied = true;
      setTimeout(() => (linkCopied = false), 1500);
    } catch {
      /* the link is selectable: copy it by hand */
    }
  }
  const states: Record<string, string> = { waiting: 'Waiting for their reply', connecting: 'Connecting…', asking: 'Wants to join', connected: 'In the session', closed: 'Left' };
</script>

{#if p2p.dialog}
  <div class="veil" role="presentation" onclick={close}>
    <div class="box" role="dialog" aria-modal="true" aria-label="Share live" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === 'Escape' && close()}>
      <div class="head">
        <h2>Share “{cave.name || 'this cave'}” live</h2>
        <button class="x" aria-label="Close" onclick={close}><X size={16} /></button>
      </div>
      <p class="note">No server in between: your devices connect straight to each other. You swap two short codes in any message, once per person. Everyone then sees each other’s edits as they happen.</p>
      <label class="name">Your name <input value={me.name} maxlength="40" onchange={(e) => rename(e.currentTarget.value)} /></label>

      {#if p2p.role !== 'host'}
        <button class="go" onclick={startHosting} disabled={!cave.hasCave}>Start sharing</button>
        {#if !cave.hasCave}<p class="err">Open or start a cave first.</p>{/if}
      {:else}
        <div class="people" aria-label="People">
          <span class="lead">Here now</span>
          {#each live.people as p}<span class="chip" style="--c:{p.color}">{p.name}{p.id === live.me ? ' (you)' : ''}</span>{/each}
        </div>
        {#each p2p.invites as inv, n (inv.id)}
          <div class="invite" class:done={inv.state === 'closed'}>
            <div class="top">
              <b>{inv.name || `Invite ${n + 1}`}</b> <span class="state">{states[inv.state]}</span>
              <span class="grow"></span>
              {#if inv.state === 'asking'}
                <button class="go small" onclick={() => letIn(inv.id)}>Let in</button>
                <button class="small" onclick={() => turnAway(inv.id)}>Turn away</button>
              {:else}
                <button class="small" onclick={() => remove(inv.id)}>{inv.state === 'connected' ? 'Remove' : 'Cancel'}</button>
              {/if}
            </div>
            {#if inv.state === 'waiting' || inv.state === 'connecting'}
              <p class="step">1. Send them this invite code.</p>
              <div class="code">
                <textarea readonly rows="3" value={inv.code || 'Making the code…'} aria-label="Invite code" onfocus={(e) => e.currentTarget.select()}></textarea>
                <button class="small" onclick={() => copy(inv.id, inv.code)} disabled={!inv.code}><Copy size={13} /> {copied === inv.id ? 'Copied' : 'Copy'}</button>
              </div>
              {#if inv.state === 'waiting'}
                <p class="step">2. Paste the reply code they send back.</p>
                <div class="code">
                  <textarea rows="2" placeholder="CAVE1-…" bind:value={replies[inv.id]} aria-label="Their reply code"></textarea>
                  <button class="small go" onclick={() => connect(inv.id)} disabled={!(replies[inv.id] ?? '').trim()}>Connect</button>
                </div>
                {#if problems[inv.id]}<p class="err" role="alert">{problems[inv.id]}</p>{/if}
              {/if}
            {/if}
          </div>
        {/each}
        <div class="invite">
          <div class="top"><b>Invite by link</b> <span class="state">one link for everyone</span></div>
          {#if p2p.link}
            <div class="code">
              <textarea readonly rows="2" value={p2p.link} aria-label="Invite link" onfocus={(e) => e.currentTarget.select()}></textarea>
              <button class="small" onclick={copyLink}><Copy size={13} /> {linkCopied ? 'Copied' : 'Copy'}</button>
            </div>
            <p class="step">Anyone who opens it joins this session. They show up above as &ldquo;Wants to join&rdquo; unless you let people in without asking (below).</p>
          {:else}
            <p class="step">No codes to swap: you send one link, and each person who opens it is connected to you. It needs the class mailbox.</p>
            <div class="row"><button class="small go" onclick={startLinkSharing}>Make a link</button></div>
            <MailboxSettings />
          {/if}
        </div>
        <div class="row">
          <button onclick={makeInvite}>Invite {p2p.invites.length ? 'another person' : 'someone'} with codes</button>
          <span class="grow"></span>
          <button onclick={() => { leaveP2p(); close(); }}>Stop sharing</button>
        </div>
      {/if}

      <label class="check"><input type="checkbox" bind:checked={p2p.auto} /> Let people in without asking</label>
      <label class="check" title="A public STUN service tells each device its public address so they can find each other across different networks. No cave data goes through it. Turn off to connect only on the same network."><input type="checkbox" bind:checked={p2p.stun} /> Connect across the internet (looks up this device’s public address)</label>
      <RelaySettings />
      <p class="fine">A code holds this device’s network address: send it only to the person you are inviting. Everything between devices is encrypted.</p>
      {#if p2p.error}<p class="err" role="alert">{p2p.error}</p>{/if}
    </div>
  </div>
{/if}

<style>
  .veil { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); display: grid; place-items: center; z-index: 45; }
  .box { width: min(560px, 94vw); max-height: 92vh; overflow: auto; background: var(--panel); color: var(--text); border: 1px solid var(--line); border-radius: 10px; padding: 16px 20px; display: flex; flex-direction: column; gap: 10px; outline: none; }
  .head { display: flex; align-items: center; }
  h2 { margin: 0; font-size: 17px; flex: 1; }
  .x { background: none; border: none; color: var(--muted); cursor: pointer; }
  .note, .fine { margin: 0; font-size: 12px; color: var(--muted); }
  .fine { font-size: 11px; }
  .name { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--muted); }
  input:not([type]) { font: inherit; font-size: 13px; padding: 5px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); }
  textarea { flex: 1; font: 11px/1.3 var(--mono, monospace); padding: 6px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); resize: vertical; word-break: break-all; }
  .people { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .lead { font-size: 12px; color: var(--muted); }
  .chip { padding: 2px 9px; border-radius: 999px; border: 1px solid var(--c); font-size: 12px; }
  .invite { border: 1px solid var(--line); border-radius: 8px; padding: 8px 10px; display: flex; flex-direction: column; gap: 6px; }
  .invite.done { opacity: 0.55; }
  .top { display: flex; align-items: center; gap: 8px; font-size: 13px; }
  .state { color: var(--muted); font-size: 12px; }
  .step { margin: 0; font-size: 12px; }
  .code { display: flex; gap: 6px; align-items: flex-start; }
  .row { display: flex; align-items: center; gap: 8px; }
  .grow { flex: 1; }
  .check { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--muted); }
  .err { margin: 0; color: var(--danger); font-size: 12px; }
  button { display: inline-flex; align-items: center; gap: 4px; font: inherit; font-size: 13px; padding: 6px 12px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); cursor: pointer; }
  button.small { font-size: 12px; padding: 3px 9px; }
  button:disabled { opacity: 0.5; cursor: default; }
  button.go { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
