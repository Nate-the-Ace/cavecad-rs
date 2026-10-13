<script lang="ts">
  // The two doors into a class. "Start a Classroom" (the teacher: a name for the class and for themselves)
  // and the page a student sees when they open the teacher's link (their name, then Join). Neither can be
  // reached by someone who is only watching, and both are outside the workspace.
  import { GraduationCap, X } from '@lucide/svelte';
  import { cave } from '../lib/cave.svelte';
  import { classroom, joinClass, startClass } from '../lib/classroom.svelte';
  import { live, me } from '../lib/sync.svelte';
  import { mailboxConfig, saveMailboxConfig } from '../lib/signal';

  let className = $state('');
  let teacherName = $state('');
  let studentName = $state('');
  // The hosted copy has no server to run a class on; anywhere else the server (a link to hand out) is the easy way.
  let via = $state<'relay' | 'link' | 'codes'>(location.hostname.endsWith('github.io') ? 'link' : 'relay');
  const start = () => {
    if (via === 'link' && mbEndpoint.trim() && mbKey.trim()) saveMailboxConfig({ endpoint: mbEndpoint, key: mbKey });
    startClass(className, teacherName, via);
  };
  // The class mailbox (CaveCAD's Apps Script): built into this copy, or typed in here and kept on this machine.
  const built = mailboxConfig();
  let mbEndpoint = $state(built?.endpoint ?? '');
  let mbKey = $state(built?.key ?? '');
  const needMailbox = $derived(via === 'link' && !(mbEndpoint.trim() && mbKey.trim()));
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
      <fieldset class="via">
        <legend>How students join</legend>
        <label class="opt"><input type="radio" bind:group={via} value="relay" /> <span><b>A link, through this server.</b> One link for everyone; the class&rsquo;s traffic goes through the server. Students need to reach it.</span></label>
        <label class="opt"><input type="radio" bind:group={via} value="link" /> <span><b>A link, then direct.</b> One link for everyone; the class&rsquo;s mailbox only passes the first hello, then each student&rsquo;s device connects straight to yours. No server carries the class.</span></label>
        <label class="opt"><input type="radio" bind:group={via} value="codes" /> <span><b>Direct, with codes.</b> No mailbox at all: you swap two short codes with each student, once.</span></label>
      </fieldset>
      {#if via === 'link' && (!built || built.source === 'local')}
        <div class="mail">
          <label>Class mailbox address <input bind:value={mbEndpoint} placeholder="https://script.google.com/macros/s/…/exec" /></label>
          <label>Mailbox key <input bind:value={mbKey} type="password" autocomplete="off" /></label>
          <span class="fine">Kept on this machine. {built ? '' : 'This copy was built without one.'}</span>
        </div>
      {/if}
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
      <div class="head"><h2><GraduationCap size={18} /> Join the class</h2></div>
      <p class="note">You are joining {classroom.pending.room ? '' : 'a class'}{#if classroom.pending.room}<b>{classroom.pending.room}</b>{/if} as an observer. You will see what the teacher sees, and the teacher will see where your pointer is. You can raise your hand and chat to ask a question.</p>
      <label>Your name <input bind:value={studentName} maxlength="40" placeholder="So the teacher knows who you are" onkeydown={(e) => e.key === 'Enter' && joinClass(studentName)} /></label>
      {#if why}<p class="err" role="alert">{why}</p>{/if}
      <div class="row">
        <button class="go" disabled={!studentName.trim() || classroom.busy} onclick={() => joinClass(studentName)}>{classroom.busy ? 'Joining…' : 'Join as an observer'}</button>
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
  .mail { display: flex; flex-direction: column; gap: 6px; }
  .via { border: 1px solid var(--line); border-radius: 8px; margin: 0; padding: 6px 10px; display: flex; flex-direction: column; gap: 4px; }
  .via legend { font-size: 12px; color: var(--muted); padding: 0 4px; }
  .opt { flex-direction: row; align-items: flex-start; gap: 6px; font-size: 12px; color: var(--text); }
  .opt input { margin-top: 2px; }
  .err { margin: 0; color: var(--danger); font-size: 12px; }
  .row { display: flex; justify-content: flex-end; gap: 8px; }
  button { font: inherit; font-size: 13px; padding: 6px 14px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  button.go { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
