<script lang="ts">
  // Sign-in is required. Until the account is known this covers the whole page: loading, the first-sign-in
  // welcome, not invited, removed, or signed out. Offline it shows a strip instead and lets the app run.
  import { account, finishWelcome } from '../../lib/account.svelte';
  import { isDesktop } from '../../lib/desktop';
  import { startDesktopSignIn } from '../../lib/desktop-signin';

  let { openDrives }: { openDrives: () => void } = $props();
  let name = $state(account.member?.displayName ?? '');
  $effect(() => { if (account.member && !name) name = account.member.displayName; });
  let busy = $state(false);
  let error = $state('');

  const REASONS: Record<string, string> = {
    none: "which hasn't been invited. If you were invited at another address, sign in with that one.",
    expired: `but this invite has expired. Ask ${account.inviter || 'whoever invited you'} for a new one.`,
    used: 'but this invite was already used by another account.',
    cancelled: `but this invite was cancelled. Ask ${account.inviter || 'whoever invited you'} if that's a mistake.`,
    'other-email': "but this invite is for a different email. Sign in with the address the invite was sent to.",
  };
  async function done(thenDrives: boolean) {
    busy = true;
    error = '';
    try {
      await finishWelcome(name.trim() || account.member!.displayName);
      if (thenDrives) openDrives();
    } catch {
      error = "Couldn't save that. Check your connection and try again.";
    } finally {
      busy = false;
    }
  }
  // Signing in again, or as someone else, goes through TinyAuth: a reload of the page after its sign-out.
  const signInAgain = () => (isDesktop() ? void startDesktopSignIn().catch(() => (error = "Couldn't open the sign-in. Check your connection and try again.")) : location.reload());
  const otherAccount = () => (location.href = (import.meta.env.VITE_SIGNOUT_URL as string | undefined) || '/');
</script>

{#if account.phase === 'offline'}
  <div class="strip" role="status">Offline: working on what's on this device</div>
{:else if account.phase === 'offline-expired'}
  <div class="strip warn" role="status">You've been offline over 30 days. Connect once to keep editing.</div>
{:else if account.phase !== 'ready'}
  <div class="gate">
    <div class="card">
      {#if account.phase === 'loading'}
        <p class="hint">Opening CaveCAD…</p>
      {:else if account.phase === 'welcome'}
        <h2>Welcome, {account.member?.displayName}</h2>
        <div class="note">
          <strong>Your caves stay yours</strong>
          <p>Your caves live on your devices and in the drives you connect. CaveCAD's server never has your surveys, sketches, scans or locations in a form it can read.</p>
          <p>For live sessions, the server keeps a locked history of edits so people can catch up. Only the people you share a cave with hold the key. The server, and its admins, can't open it.</p>
          <p>The server knows your name, email, which drives you use, and when edits happened.</p>
        </div>
        <label class="lab" for="welcome-name">Name shown to others</label>
        <input id="welcome-name" class="field" bind:value={name} maxlength="40" />
        <span class="lab">Where are your caves?</span>
        <div class="row">
          <button class="btn primary" disabled={busy} onclick={() => done(true)}>Connect a drive</button>
          <button class="btn" disabled={busy} onclick={() => done(false)}>Keep on this device for now</button>
        </div>
      {:else if account.phase === 'not-invited'}
        <h2>CaveCAD is invite only for now</h2>
        <p>You're signed in as {account.email}, {REASONS[account.reason] ?? REASONS.none}</p>
        <div class="row"><button class="btn" onclick={otherAccount}>Use another account</button></div>
      {:else if account.phase === 'removed'}
        <h2>Your CaveCAD account was removed</h2>
        <p>You're signed in as {account.email}. Ask an admin if this is a mistake.</p>
        <div class="row"><button class="btn" onclick={otherAccount}>Use another account</button></div>
      {:else}
        <h2>Sign in to CaveCAD</h2>
        <p>Sign in with your Google or Microsoft account.</p>
        <div class="row"><button class="btn primary" onclick={signInAgain}>Sign in</button></div>
      {/if}
      {#if error}<p class="hint bad">{error}</p>{/if}
    </div>
  </div>
{/if}

<style>
  .gate { position: fixed; inset: 0; z-index: 100; display: grid; place-items: center; padding: 16px; background: var(--bg); }
  .card { width: min(460px, 100%); display: flex; flex-direction: column; gap: 10px; padding: 20px 22px; background: var(--panel); border: 1px solid var(--line); border-radius: 12px; color: var(--text); }
  h2 { margin: 0; font-size: 18px; color: var(--text-strong); }
  p { margin: 0; line-height: 1.5; }
  .note { display: flex; flex-direction: column; gap: 6px; padding: 12px 14px; background: var(--accent-bg); border-radius: var(--radius); font-size: 13px; }
  .lab { color: var(--muted); font-size: 12px; }
  .field { font: inherit; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); padding: 6px 8px; }
  .row { display: flex; gap: 8px; flex-wrap: wrap; }
  .btn { padding: 6px 12px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); cursor: pointer; font: inherit; }
  .btn.primary { background: var(--accent); color: #fff; border-color: var(--accent); }
  .btn:disabled { opacity: 0.5; }
  .hint { color: var(--muted); font-size: 12px; }
  .bad { color: var(--danger); }
  .strip { position: fixed; left: 50%; bottom: 8px; transform: translateX(-50%); z-index: 70; padding: 4px 12px; border-radius: var(--radius); background: var(--panel); border: 1px solid var(--line); color: var(--muted); font-size: 12px; }
  .strip.warn { color: var(--danger); border-color: var(--danger); }
</style>
