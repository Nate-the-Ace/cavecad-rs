<script lang="ts">
  // The phone Settings tab's Account section: who is signed in (name and email) and the same Account section the
  // desktop's user menu shows (Sign out, display name, colour), tightened to the phone's spacing.
  import AccountSection from './account/AccountSection.svelte';
  import { account } from '../lib/account.svelte';
</script>

<div class="acct">
  {#if account.member}
    <div class="who"><b>{account.member.displayName}</b><span>{account.member.email}</span></div>
    <AccountSection />
  {:else}
    <p class="who">Not signed in</p>
  {/if}
</div>

<style>
  .acct { display: flex; flex-direction: column; padding: var(--p-pad); gap: var(--p-gap); }
  .who { display: flex; flex-direction: column; margin: 0; }
  .who b { font-size: 15px; }
  .who span { color: var(--muted); font-size: var(--p-font-small); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .acct :global(.pane) { padding: 0; gap: var(--p-gap); }
  .acct :global(.row) { min-height: var(--p-row); }
  .acct :global(.btn) { min-height: var(--p-row); padding: 0 12px; font-size: var(--p-font); }
  .acct :global(.sw) { width: 28px; height: 28px; }
  .acct :global(.field) { font-size: var(--p-font-input); }
</style>
