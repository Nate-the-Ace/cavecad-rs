<script lang="ts">
  import RelaySettings from './RelaySettings.svelte';
  import MailboxSettings from './MailboxSettings.svelte';
  // Sharing the open cave live, straight to other devices. One link is made for the session; anyone who opens it
  // (or pastes it into their CaveCAD) is introduced to this page and then connects directly. A small mailbox only
  // passes the first hello. Everyone then sees each other's edits, pointers and (if they follow) where the host looks.
  import { Copy, X } from '@lucide/svelte';
  import { cave } from '../lib/cave.svelte';
  import { leaveP2p, letIn, p2p, remove, startLinkSharing, turnAway } from '../lib/p2p.svelte';
  import { configuredMailbox } from '../lib/signal';
  import { live, me, rename } from '../lib/sync.svelte';

  let hasMailbox = $state(!!configuredMailbox());
  let linkCopied = $state(false);

  const close = () => (p2p.dialog = false);
  async function copyLink() {
    try {
      await navigator.clipboard.writeText(p2p.link);
      linkCopied = true;
      setTimeout(() => (linkCopied = false), 1500);
    } catch {
      /* the link is selectable: copy it by hand */
    }
  }
  let appCopied = $state(false);
  async function copyAppLink() {
    try {
      await navigator.clipboard.writeText(p2p.appLink);
      appCopied = true;
      setTimeout(() => (appCopied = false), 1500);
    } catch {
      /* selectable: copy it by hand */
    }
  }
  const states: Record<string, string> = { waiting: 'Connecting…', connecting: 'Connecting…', asking: 'Wants to join', connected: 'In the session', closed: 'Left' };
  /** The people the link has brought, newest last (an unused invite the mailbox made ahead is not one of them). */
  const arrivals = $derived(p2p.invites.filter((i) => i.name || i.state === 'asking' || i.state === 'connected'));
</script>

{#if p2p.dialog}
  <div class="veil" role="presentation" onclick={close}>
    <div class="box" role="dialog" aria-modal="true" aria-label="Share live" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === 'Escape' && close()}>
      <div class="head">
        <h2>Share “{cave.name || 'this cave'}” live</h2>
        <button class="x" aria-label="Close" onclick={close}><X size={16} /></button>
      </div>
      <p class="note">One link for everyone. Send it to the people you want to work with: when they open it (or paste it into their CaveCAD), they are connected straight to you, with no server in between, and see each other&rsquo;s edits as they happen.</p>
      <label class="name">Your name <input value={me.name} maxlength="40" onchange={(e) => rename(e.currentTarget.value)} /></label>

      {#if p2p.role !== 'host' || !p2p.link}
        <button class="go" onclick={() => { startLinkSharing(); hasMailbox = !!configuredMailbox(); }} disabled={!cave.hasCave}>Make the link</button>
        {#if !cave.hasCave}<p class="err">Open or start a cave first.</p>{/if}
        {#if !hasMailbox}
          <p class="step">This copy of CaveCAD was built without the mailbox that starts a connection. Enter it once; it is kept on this computer.</p>
          <MailboxSettings onsave={() => (hasMailbox = !!configuredMailbox())} />
        {/if}
      {:else}
        <div class="invite">
          <div class="top"><b>Your link</b> <span class="state">send it to anyone you want in</span></div>
          <div class="code">
            <textarea readonly rows="2" value={p2p.link} aria-label="Invite link" onfocus={(e) => e.currentTarget.select()}></textarea>
            <button class="small" onclick={copyLink}><Copy size={13} /> {linkCopied ? 'Copied' : 'Copy'}</button>
          </div>
          <p class="step">Opens in the browser. For people who have the CaveCAD desktop app, this one opens the app instead:</p>
          <div class="code">
            <textarea readonly rows="1" value={p2p.appLink} aria-label="Link that opens the desktop app" onfocus={(e) => e.currentTarget.select()}></textarea>
            <button class="small" onclick={copyAppLink}><Copy size={13} /> {appCopied ? 'Copied' : 'Copy'}</button>
          </div>
        </div>
        <div class="people" aria-label="People">
          <span class="lead">Here now</span>
          {#each live.people as p}<span class="chip" style="--c:{p.color}">{p.name}{p.id === live.me ? ' (you)' : ''}</span>{/each}
        </div>
        {#each arrivals as inv, n (inv.id)}
          <div class="invite" class:done={inv.state === 'closed'}>
            <div class="top">
              <b>{inv.name || `Guest ${n + 1}`}</b> <span class="state">{states[inv.state]}</span>
              <span class="grow"></span>
              {#if inv.state === 'asking'}
                <button class="go small" onclick={() => letIn(inv.id)}>Let in</button>
                <button class="small" onclick={() => turnAway(inv.id)}>Turn away</button>
              {:else}
                <button class="small" onclick={() => remove(inv.id)}>{inv.state === 'connected' ? 'Remove' : 'Cancel'}</button>
              {/if}
            </div>
          </div>
        {/each}
        <div class="row">
          <span class="grow"></span>
          <button onclick={() => { leaveP2p(); close(); }}>Stop sharing</button>
        </div>
      {/if}

      <label class="check"><input type="checkbox" bind:checked={p2p.auto} /> Let people in without asking</label>
      <label class="check" title="A public STUN service tells each device its public address so they can find each other across different networks. No cave data goes through it. Turn off to connect only on the same network."><input type="checkbox" bind:checked={p2p.stun} /> Connect across the internet (looks up this device’s public address)</label>
      <RelaySettings />
      <p class="fine">Anyone with the link can ask to join. Everything between devices is encrypted.</p>
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
