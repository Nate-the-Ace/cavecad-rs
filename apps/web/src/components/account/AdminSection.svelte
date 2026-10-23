<script lang="ts">
  // Admin: who may sign up, invites, and members. Only admins see it; the server enforces every rule again.
  import { onMount } from 'svelte';
  import { account, api } from '../../lib/account.svelte';
  import { AccountError, type Invite, type Member, type Role } from '../../lib/account-api';

  let members = $state<Member[]>([]);
  let invites = $state<Invite[]>([]);
  let email = $state('');
  let role = $state<Role>('member');
  let result = $state<{ email: string; link: string; emailed: boolean } | null>(null);
  let links = $state<Record<string, string>>({});
  let removing = $state<string | null>(null);
  let error = $state('');
  let copied = $state('');

  const WORDS: Record<string, string> = {
    'last-admin': 'CaveCAD needs at least one admin.',
    'already-member': "They're already a member.",
    self: "You can't change your own role or remove yourself.",
    'bad-email': "That doesn't look like an email address.",
    'not-admin': 'Only admins can do that.',
  };
  async function run(f: () => Promise<void>) {
    error = '';
    try {
      await f();
    } catch (e) {
      error = e instanceof AccountError ? WORDS[e.code] ?? "Couldn't do that. Try again." : "Couldn't reach CaveCAD. Check your connection.";
    }
  }
  const refresh = () => run(async () => { [members, invites] = await Promise.all([api.members(), api.invites()]); });
  onMount(refresh);

  const ago = (t: number | null) => {
    if (!t) return 'never';
    const d = Math.floor((Date.now() - t) / 86_400_000);
    return d === 0 ? 'today' : d === 1 ? 'yesterday' : d < 14 ? `${d} days ago` : `${Math.round(d / 7)} weeks ago`;
  };
  const left = (t: number) => `${Math.max(0, Math.ceil((t - Date.now()) / 86_400_000))} days`;
  const initials = (n: string) => n.split(/\s+/).filter(Boolean).map((w) => w[0]).slice(0, 2).join('').toUpperCase();
  async function copy(text: string) {
    await navigator.clipboard.writeText(text).catch(() => {});
    copied = text;
    setTimeout(() => copied === text && (copied = ''), 2500);
  }

  const send = () => run(async () => {
    const made = await api.invite(email.trim(), role);
    result = { email: made.invite.email, link: made.link, emailed: made.emailed };
    links[made.invite.id] = made.link;
    email = '';
    await refresh();
  });
  const resend = (id: string) => run(async () => { const made = await api.resend(id); links[made.invite.id] = made.link; await refresh(); });
  const cancel = (id: string) => run(async () => { await api.cancel(id); await refresh(); });
  const changeRole = (id: string, r: Role) => run(async () => { await api.setRole(id, r); await refresh(); });
  const remove = (id: string) => run(async () => { await api.removeMember(id); removing = null; await refresh(); });
  const setSignups = (v: 'invite' | 'open') => run(async () => { await api.setSignups(v); account.signups = v; });
</script>

<div class="pane">
  <div class="row">
    <div class="grow"><div class="sec">Sign-ups</div><div class="hint">{account.signups === 'invite' ? 'Only people you invite can make an account.' : 'Anyone with a Google or Microsoft account can join.'}</div></div>
    <select value={account.signups} onchange={(e) => setSignups(e.currentTarget.value as 'invite' | 'open')} aria-label="Who can sign up">
      <option value="invite">Invite only</option>
      <option value="open">Open to anyone</option>
    </select>
  </div>

  <div class="sec">Invite someone</div>
  <div class="row">
    <input class="field grow" type="email" bind:value={email} placeholder="caver@example.com" aria-label="Their email" />
    <select bind:value={role} aria-label="Their role"><option value="member">Member</option><option value="admin">Admin</option></select>
    <button class="btn" onclick={send} disabled={!email.trim()}>Send invite</button>
  </div>
  <p class="hint">They get an email from CaveCAD with a link. It works once, for that email only, for 7 days.</p>
  {#if result}
    <div class="note">
      {#if result.emailed}Invite emailed to {result.email}.{:else}The email didn't send. Copy the link and send it yourself.{/if}
      <div class="row"><input class="field grow" readonly value={result.link} aria-label="Invite link" /><button class="btn" onclick={() => copy(result!.link)}>{copied === result.link ? 'Copied' : 'Copy link'}</button></div>
    </div>
  {/if}

  {#if invites.length}
    <div class="sec">Waiting to join</div>
    <ul class="list">
      {#each invites as i (i.id)}
        <li>
          <div class="grow"><div>{i.email}</div><div class="hint">{i.role === 'admin' ? 'Admin' : 'Member'} · sent {ago(i.createdAt)} · expires in {left(i.expiresAt)}</div></div>
          {#if links[i.id]}<button class="btn" onclick={() => copy(links[i.id])}>{copied === links[i.id] ? 'Copied' : 'Copy link'}</button>{/if}
          <button class="btn" onclick={() => resend(i.id)}>Resend</button>
          <button class="btn" onclick={() => cancel(i.id)}>Cancel</button>
        </li>
      {/each}
    </ul>
  {/if}

  <div class="sec">Members ({members.length})</div>
  <ul class="list">
    {#each members as m (m.id)}
      <li>
        <span class="face" style="--c:{m.colour}">{initials(m.displayName)}</span>
        <div class="grow">
          <div>{m.displayName}{m.id === account.member?.id ? ' (you)' : ''}</div>
          <div class="hint">{m.role === 'admin' ? 'Admin' : 'Member'}{m.provider ? ` · ${m.provider === 'google' ? 'Google' : 'Microsoft'}` : ''} · last seen {ago(m.lastSeenAt)}{m.invitedByName ? ` · invited by ${m.invitedBy === account.member?.id ? 'you' : m.invitedByName}` : ''}</div>
        </div>
        {#if m.id !== account.member?.id}
          {#if removing === m.id}
            <span class="hint">Remove {m.displayName}?</span>
            <button class="btn" onclick={() => (removing = null)}>Keep</button>
            <button class="btn danger" onclick={() => remove(m.id)}>Remove</button>
          {:else}
            <select value={m.role} onchange={(e) => changeRole(m.id, e.currentTarget.value as Role)} aria-label="Role of {m.displayName}"><option value="member">Member</option><option value="admin">Admin</option></select>
            <button class="btn" onclick={() => (removing = m.id)}>Remove</button>
          {/if}
        {/if}
      </li>
    {/each}
  </ul>
  {#if error}<p class="hint bad" role="alert">{error}</p>{/if}
</div>

<style>
  .pane { display: flex; flex-direction: column; gap: 10px; padding: 12px 14px 14px; }
  .row { display: flex; align-items: center; gap: 8px; }
  .grow { flex: 1; min-width: 0; }
  .sec { font-weight: 600; color: var(--text-strong); }
  .list { list-style: none; margin: 0; padding: 0; border: 1px solid var(--line); border-radius: var(--radius); }
  .list li { display: flex; align-items: center; gap: 8px; padding: 8px 10px; }
  .list li + li { border-top: 1px solid var(--line); }
  .face { width: 26px; height: 26px; flex-shrink: 0; border-radius: 50%; background: var(--c); color: #fff; display: grid; place-items: center; font-size: 11px; font-weight: 600; }
  .note { display: flex; flex-direction: column; gap: 6px; padding: 8px 10px; background: var(--accent-bg); border-radius: var(--radius); }
  .field, select { font: inherit; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); padding: 5px 8px; line-height: 1.3; }
  .btn { display: inline-flex; align-items: center; gap: 6px; padding: 5px 10px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); cursor: pointer; font: inherit; font-size: 12px; white-space: nowrap; }
  .btn:hover:not(:disabled) { background: var(--accent-bg); }
  .btn:disabled { opacity: 0.5; cursor: default; }
  .btn.danger { color: var(--danger); border-color: var(--danger); }
  .hint { margin: 0; color: var(--muted); font-size: 12px; }
  .bad { color: var(--danger); }
</style>
