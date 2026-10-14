<script lang="ts">
  // The two doors into a class. "Start a Classroom" (the teacher: a name for the class and for themselves)
  // and the page a student sees when they open the teacher's link (their name, then Join). Neither can be
  // reached by someone who is only watching, and both are outside the workspace.
  import { GraduationCap, X } from '@lucide/svelte';
  import { cave } from '../lib/cave.svelte';
  import { classroom, joinClass, startClass } from '../lib/classroom.svelte';
  import { live, me } from '../lib/sync.svelte';
  import { p2p } from '../lib/p2p.svelte';
  import { isDesktop } from '../lib/desktop';
  import { desktopLink } from '../lib/classroom-log';
  import MailboxSettings from './MailboxSettings.svelte';
  import { mailboxConfig } from '../lib/signal';

  let className = $state('');
  let teacherName = $state('');
  let studentName = $state('');
  // How long joining has taken, so a wait is a number and not a mystery.
  let waited = $state(0);
  /** The same join as a link that opens the CaveCAD desktop app, for someone who has it. */
  const appUrl = $derived(classroom.pending && 'live' in classroom.pending && classroom.pending.live ? desktopLink(classroom.pending.live.session, classroom.pending.live.mailbox ?? undefined) : '');
  $effect(() => {
    if (!classroom.busy || !classroom.pending) return void (waited = 0);
    const t0 = Date.now();
    const t = setInterval(() => (waited = Math.round((Date.now() - t0) / 1000)), 500);
    return () => clearInterval(t);
  });
  // The cave is only shown to students: by default none of it stays on their computers.
  let keepOff = $state(true);
  const start = () => startClass(className, teacherName, 'link', { private: keepOff });
  // The class mailbox (CaveCAD's Apps Script): built into this copy, or typed in and kept on this machine.
  let hasMailbox = $state(!!mailboxConfig());
  const needMailbox = $derived(!hasMailbox);
  $effect(() => {
    if (classroom.dialog && !teacherName) teacherName = me.name;
  });
  const close = () => ((classroom.dialog = false), (classroom.error = ''));
  const why = $derived(classroom.error || live.error);
</script>

{#if classroom.dialog && classroom.role === 'none'}
  <div class="veil" role="presentation" data-classroom-ui onclick={close}>
    <div class="box" role="dialog" aria-modal="true" aria-label="Start a Classroom" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === 'Escape' && close()}>
      <div class="head">
        <h2><GraduationCap size={18} /> Start a Classroom</h2>
        <button class="x" aria-label="Close" onclick={close}><X size={16} /></button>
      </div>
      <p class="note">You teach from “{cave.name || 'the open cave'}”. Students open a link and watch your screen: they see what you see, as you do it, and cannot change anything. Their pointers show to you. A raised hand and a chat let them ask questions, and every question is kept in the class&rsquo;s record.</p>
      <label>Name of the class <input bind:value={className} placeholder="Caving 101, Truitt survey day…" maxlength="60" onkeydown={(e) => e.key === 'Enter' && start()} /></label>
      <label>Your name <input bind:value={teacherName} maxlength="40" /></label>
      <p class="note">Students open one link. A small mailbox only passes their first hello; then each student's device connects straight to yours, so no server carries the class.</p>
      <label class="opt"><input type="checkbox" bind:checked={keepOff} /> <span><b>Students cannot keep the cave.</b> They can only look: saving, exporting and printing are off for them, and the cave and its scans are cleared from their computers when they leave or the class ends. Turn this off if they may keep a copy.</span></label>
      <MailboxSettings onsave={() => (hasMailbox = !!mailboxConfig())} />
      {#if !cave.hasCave}<p class="err">Open or start a cave first: there is nothing to teach from yet.</p>{/if}
      {#if why}<p class="err" role="alert">{why}</p>{/if}
      <p class="fine">What you type in the chat is kept in the class&rsquo;s record on the server and in the file you can download, so do not put a cave&rsquo;s entrance location in it.</p>
      <div class="row">
        <button onclick={close}>Cancel</button>
        <button class="go" disabled={!className.trim() || !cave.hasCave || classroom.busy || needMailbox} onclick={start}>{classroom.busy ? 'Starting…' : 'Start the class'}</button>
      </div>
    </div>
  </div>
{/if}

{#if classroom.pending && classroom.role === 'none'}
  <div class="veil full" role="presentation" data-classroom-ui>
    <div class="box" role="dialog" aria-modal="true" aria-label="Join the class">
      <div class="head"><h2><GraduationCap size={18} /> {classroom.pending.live && !classroom.pending.room ? 'Join the session' : 'Join the class'}</h2></div>
      {#if classroom.pending.live && !classroom.pending.room}
        <p class="note">You were sent a link to a shared CaveCAD session. Type your name and join; the person who sent it may need to let you in first. Whether you can edit or only watch is up to them.</p>
      {:else}
      <p class="note">You are joining {classroom.pending.room ? '' : 'a class'}{#if classroom.pending.room}<b>{classroom.pending.room}</b>{/if} as an observer. You will see what the teacher sees, and the teacher will see where your pointer is. You can raise your hand and chat to ask a question.</p>
      {/if}
      <label>Your name <input bind:value={studentName} maxlength="40" placeholder="So the others know who you are" onkeydown={(e) => e.key === 'Enter' && joinClass(studentName)} /></label>
      {#if classroom.busy}<p class="note" role="status">{p2p.waiting || 'Joining…'} {waited ? `${waited} s` : ''}</p>{/if}
      {#if why}<p class="err" role="alert">{why}</p>{/if}
      <div class="row">
        {#if appUrl && !isDesktop()}<a class="app" href={appUrl} title="Opens the CaveCAD desktop app, if you have it, and joins from there">Open in the desktop app</a>{/if}
        <button class="go" disabled={!studentName.trim() || classroom.busy} onclick={() => joinClass(studentName)}>{classroom.busy ? 'Joining…' : classroom.pending.live && !classroom.pending.room ? 'Join' : 'Join as an observer'}</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .veil { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); display: grid; place-items: center; z-index: 60; }
  .veil.full { background: var(--bg); }
  .box { width: min(480px, 94vw); background: var(--panel); color: var(--text); border: 1px solid var(--line); border-radius: 10px; padding: 16px 20px; display: flex; flex-direction: column; gap: 10px; outline: none; }
  .head { display: flex; align-items: center; }
  h2 { margin: 0; font-size: 17px; flex: 1; display: flex; align-items: center; gap: 8px; }
  .x { background: none; border: none; color: var(--muted); cursor: pointer; }
  .note, .fine { margin: 0; font-size: 12px; color: var(--muted); }
  .fine { font-size: 11px; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--muted); }
  input { font: inherit; font-size: 14px; padding: 6px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--field); color: var(--text); }
  .opt { flex-direction: row; align-items: flex-start; gap: 6px; font-size: 12px; color: var(--text); }
  .opt input { margin-top: 2px; }
  .err { margin: 0; color: var(--danger); font-size: 12px; }
  .row { display: flex; justify-content: flex-end; gap: 8px; }
  button { font: inherit; font-size: 13px; padding: 6px 14px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  button.go { background: var(--accent); border-color: var(--accent); color: #fff; }
  .app { align-self: center; margin-right: auto; font-size: 12px; color: var(--muted); }
</style>
