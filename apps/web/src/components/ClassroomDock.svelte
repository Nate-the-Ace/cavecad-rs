<script lang="ts">
  // The class's side drawer. Teacher: the link to hand out, who is here and whose hand is up (call on them),
  // the chat, Download log and End class. Student: a note that they are watching, the Raise hand button and the
  // chat. It sits outside the workspace (it is not a panel the teacher's layout can move) and is the one
  // part of the page a student can click and type in.
  import { tick } from 'svelte';
  import { ChevronRight, ChevronLeft, Hand, Copy, Download, LogOut, Square, Send } from '@lucide/svelte';
  import { callOn, classroom, downloadLog, endClass, handIsUp, leaveClass, raiseHand, say, studentLink } from '../lib/classroom.svelte';
  import { clock, waiting } from '../lib/classroom-log';
  import { live } from '../lib/sync.svelte';

  let text = $state('');
  let list: HTMLDivElement | undefined = $state();
  let copied = $state(false);
  let now = $state(Date.now());

  // How long each hand has been up counts on, and the chat stays scrolled to the newest line.
  $effect(() => {
    const t = setInterval(() => (now = Date.now()), 5000);
    return () => clearInterval(t);
  });
  $effect(() => {
    void classroom.chat.length;
    void classroom.drawer;
    tick().then(() => list && (list.scrollTop = list.scrollHeight));
  });
  $effect(() => {
    if (classroom.drawer) classroom.unread = 0;
  });

  const students = $derived(live.people.filter((p) => p.role === 'student'));
  const teacher = $derived(live.people.find((p) => p.role === 'teacher'));
  const link = $derived(studentLink());
  const up = $derived(handIsUp());
  const handOf = (id: string) => classroom.hands.find((h) => h.id === id);

  function send() {
    if (say(text)) text = '';
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(link);
      copied = true;
      setTimeout(() => (copied = false), 1500);
    } catch {
      /* the link is selectable: copy it by hand */
    }
  }

  function end() {
    if (window.confirm('End the class? Nobody can say anything more, and the record is complete. You can still download it.')) endClass();
  }
  function leaveNow() {
    if (classroom.role === 'teacher' && !classroom.ended && !window.confirm('Leave the class? Students keep what they have but stop following you until you start a class again.')) return;
    leaveClass();
  }
</script>

{#if classroom.role !== 'none'}
  {#if classroom.drawer}
    <aside class="dock" data-classroom-ui aria-label="Class">
      <header>
        <div class="title">
          <b>{classroom.name}</b>
          <span class="role">{classroom.role === 'teacher' ? 'You are the teacher' : `Watching ${teacher ? teacher.name : 'the teacher'}`}</span>
        </div>
        <button class="icon" aria-label="Hide the class panel" title="Hide" onclick={() => (classroom.drawer = false)}><ChevronRight size={16} /></button>
      </header>

      {#if classroom.ended}<p class="ended" role="status">The class has ended.</p>{/if}
      {#if classroom.role === 'student'}
        <p class="note">You are following the teacher&rsquo;s screen. You can&rsquo;t change anything; your pointer is shown to the teacher. To ask something, raise your hand and write your question in the chat.</p>
        <button class="hand" class:on={up} disabled={classroom.ended} onclick={() => raiseHand(!up)} aria-pressed={up}>
          <Hand size={16} /> {up ? 'Lower my hand' : 'Raise my hand'}
        </button>
      {:else}
        <div class="link">
          <input readonly value={link || 'Making the link…'} aria-label="Link for students" onfocus={(e) => e.currentTarget.select()} />
          <button class="small" onclick={copy} disabled={!link}><Copy size={13} /> {copied ? 'Copied' : 'Copy'}</button>
        </div>
        <p class="note">Send students this link. They watch your screen and can raise a hand.</p>
        <div class="people" aria-label="Students">
          <span class="lead">{students.length} {students.length === 1 ? 'student' : 'students'}</span>
          {#each students as p (p.id)}
            {@const h = handOf(p.id)}
            <div class="person" class:raised={!!h}>
              <span class="dot" style="background:{p.color}"></span>
              <span class="name">{p.name}</span>
              {#if h}
                <span class="wait" title="Hand up for"><Hand size={12} /> {waiting(h.since, now)}</span>
                <button class="small go" onclick={() => callOn(p.id)}>Call on</button>
              {/if}
            </div>
          {:else}
            <span class="muted">Nobody has joined yet.</span>
          {/each}
        </div>
      {/if}

      <div class="chat" bind:this={list} aria-label="Class chat" role="log">
        {#each classroom.chat as e (e.n)}
          {#if e.kind === 'say'}
            <div class="say" class:mine={e.id === live.me}><span class="when">{clock(e.at)}</span> <b class:teacher={e.role === 'teacher'}>{e.name}</b> {e.text}</div>
          {:else}
            <div class="event"><span class="when">{clock(e.at)}</span> {e.kind === 'hand' ? `${e.name} ${e.text}` : e.text}</div>
          {/if}
        {:else}
          <p class="muted">Questions and answers appear here, and are kept in the class&rsquo;s record.</p>
        {/each}
      </div>
      {#if classroom.notice}<p class="notice" role="status">{classroom.notice}</p>{/if}
      <form class="say-form" onsubmit={(e) => (e.preventDefault(), send())}>
        <input bind:value={text} maxlength="500" placeholder={classroom.role === 'student' ? 'Write your question…' : 'Say something to the class…'} aria-label="Message" disabled={classroom.ended} />
        <button class="small go" type="submit" disabled={!text.trim() || classroom.ended} aria-label="Send"><Send size={14} /></button>
      </form>

      <footer>
        {#if classroom.role === 'teacher'}
          <button class="small" onclick={downloadLog} title="Save the whole record of the class (every question and hand) as a text file"><Download size={13} /> Download log</button>
          <button class="small" onclick={end} disabled={classroom.ended}><Square size={13} /> End class</button>
        {/if}
        <span class="grow"></span>
        <button class="small" onclick={leaveNow}><LogOut size={13} /> Leave</button>
      </footer>
    </aside>
  {:else}
    <div class="strip" data-classroom-ui>
      <button class="icon" aria-label="Show the class panel" title="Show the class panel" onclick={() => (classroom.drawer = true)}>
        <ChevronLeft size={16} />
        {#if classroom.unread}<span class="badge">{classroom.unread}</span>{/if}
      </button>
      {#if classroom.role === 'student'}
        <button class="icon" class:on={up} aria-label={up ? 'Lower my hand' : 'Raise my hand'} aria-pressed={up} title={up ? 'Lower my hand' : 'Raise my hand'} disabled={classroom.ended} onclick={() => raiseHand(!up)}><Hand size={16} /></button>
      {:else if classroom.hands.length}
        <span class="badge hands" title="Hands up">{classroom.hands.length}</span>
      {/if}
    </div>
  {/if}
{/if}

<style>
  .dock { width: 300px; flex: none; display: flex; flex-direction: column; gap: 8px; padding: 10px; background: var(--panel); border-left: 1px solid var(--line); color: var(--text); font-size: 13px; min-height: 0; }
  header { display: flex; align-items: flex-start; gap: 6px; }
  .title { flex: 1; display: flex; flex-direction: column; min-width: 0; }
  .title b { font-size: 14px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .role { font-size: 12px; color: var(--muted); }
  .icon { display: inline-flex; align-items: center; gap: 2px; padding: 4px; border: none; background: none; color: var(--muted); cursor: pointer; border-radius: 6px; position: relative; }
  .icon:hover { background: var(--panel-2); color: var(--text); }
  .icon.on { color: var(--accent); background: var(--accent-bg); }
  .note { margin: 0; font-size: 12px; color: var(--muted); }
  .ended { margin: 0; padding: 4px 8px; border-radius: 6px; background: var(--accent-bg); font-size: 12px; }
  .hand { display: inline-flex; align-items: center; justify-content: center; gap: 6px; padding: 8px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); font: inherit; cursor: pointer; }
  .hand.on { background: var(--accent); border-color: var(--accent); color: #fff; }
  .hand:disabled { opacity: 0.5; cursor: default; }
  .link { display: flex; gap: 6px; }
  .link input { flex: 1; min-width: 0; font: 11px var(--mono); }
  input { font: inherit; font-size: 13px; padding: 5px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--field); color: var(--text); min-width: 0; }
  .people { display: flex; flex-direction: column; gap: 4px; max-height: 30%; overflow: auto; }
  .lead { font-size: 12px; color: var(--muted); }
  .person { display: flex; align-items: center; gap: 6px; padding: 2px 4px; border-radius: 6px; }
  .person.raised { background: var(--accent-bg); }
  .dot { width: 9px; height: 9px; border-radius: 50%; flex: none; }
  .name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .wait { display: inline-flex; align-items: center; gap: 3px; font-size: 11px; color: var(--muted); }
  .chat { flex: 1; min-height: 80px; overflow: auto; display: flex; flex-direction: column; gap: 4px; padding: 6px; border: 1px solid var(--line); border-radius: 8px; background: var(--panel-2); }
  .say { word-break: break-word; }
  .say.mine { color: var(--text-strong); }
  .say b.teacher { color: var(--accent); }
  .event { font-size: 11px; color: var(--muted); }
  .when { font-size: 10px; color: var(--dim); margin-right: 2px; }
  .muted { margin: 0; font-size: 12px; color: var(--muted); }
  .notice { margin: 0; font-size: 12px; color: var(--warn); }
  .say-form { display: flex; gap: 6px; }
  .say-form input { flex: 1; }
  footer { display: flex; align-items: center; gap: 6px; }
  .grow { flex: 1; }
  .small { display: inline-flex; align-items: center; gap: 4px; padding: 4px 9px; font: inherit; font-size: 12px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); cursor: pointer; }
  .small:disabled { opacity: 0.5; cursor: default; }
  .small.go { background: var(--accent); border-color: var(--accent); color: #fff; }
  .strip { width: 34px; flex: none; display: flex; flex-direction: column; align-items: center; gap: 6px; padding-top: 8px; background: var(--panel); border-left: 1px solid var(--line); }
  .badge { position: absolute; top: -2px; right: -4px; min-width: 15px; height: 15px; padding: 0 3px; border-radius: 999px; background: var(--danger); color: #fff; font-size: 10px; line-height: 15px; text-align: center; box-sizing: border-box; }
  .badge.hands { position: static; background: var(--accent); }
</style>
