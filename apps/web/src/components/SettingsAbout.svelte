<script lang="ts">
  // Settings > About (phone): which build this is and where it runs, and Send feedback (the same report the desktop's
  // user menu sends through lib/feedback.ts: the message, the app log and a few counts, never the cave).
  import { onMount } from 'svelte';
  import { Send } from '@lucide/svelte';
  import { engine, loadBuildInfo, loadEngine, type BuildInfo } from '../lib/engine';
  import { isDesktop } from '../lib/desktop';
  import { KIND_LABEL, buildReport, send, type FeedbackKind } from '../lib/feedback';
  import './settings.css';

  let info = $state<BuildInfo | null>(null);
  let engineVersion = $state('…');
  onMount(() => {
    void loadBuildInfo().then((i) => (info = i));
    void loadEngine()
      .then(() => (engineVersion = engine.version()))
      .catch(() => (engineVersion = 'not loaded'));
  });

  const build = $derived(info ? `${info.git}${info.dirty ? '+' : ''}` : 'unknown');
  const built = $derived(info?.builtAt ? new Date(info.builtAt).toLocaleString() : 'unknown');
  const rows = $derived<[string, string][]>([
    ['Build', build],
    ['Built', built],
    ['Engine', engineVersion],
    ['Platform', isDesktop() ? 'Desktop app' : 'Browser'],
  ]);

  let kind = $state<FeedbackKind>('bug');
  let message = $state('');
  let sending = $state(false);
  let note = $state('');
  async function submit() {
    if (!message.trim() || sending) return;
    sending = true;
    note = '';
    try {
      const r = await buildReport({ kind, message, includeLog: true, includeDetails: true, screenshot: null });
      const out = await send(r.name, r.zip);
      note = out.message;
      if (out.ok) message = '';
    } catch (e) {
      note = `Could not send: ${String(e)}`;
    } finally {
      sending = false;
    }
  }
</script>

<div class="set">
  <dl>
    {#each rows as [k, v] (k)}<dt>{k}</dt><dd>{v}</dd>{/each}
  </dl>
  <div class="row">
    <select bind:value={kind} aria-label="What kind of feedback">
      {#each Object.entries(KIND_LABEL) as [id, label] (id)}<option value={id}>{label}</option>{/each}
    </select>
    <button class="on" disabled={!message.trim() || sending} onclick={submit}><Send size={16} /> {sending ? 'Sending…' : 'Send feedback'}</button>
  </div>
  <textarea bind:value={message} rows="3" maxlength="5000" placeholder="Feedback" aria-label="Your feedback"></textarea>
  {#if note}<p class="line" role="status">{note}</p>{/if}
</div>

<style>
  dl { display: grid; grid-template-columns: auto 1fr; gap: 2px 12px; margin: 0; flex: none; font-size: var(--p-font); }
  dt { color: var(--muted); }
  dd { margin: 0; min-width: 0; overflow-wrap: anywhere; -webkit-user-select: text; user-select: text; font-variant-numeric: tabular-nums; }
  textarea { resize: none; flex: 0 1 120px; min-height: 80px; }
</style>
