<script lang="ts">
  // Get the desktop app: the installer for this computer first, the others below, each with its first-open steps.
  import { choices, platformOf, type Manifest } from '../../lib/downloads';
  let { onclose }: { onclose: () => void } = $props();
  let manifest = $state<Manifest | null>(null);
  let error = $state('');
  const UNAVAILABLE = "The desktop app isn't available to download right now.";
  const mine = platformOf(navigator.userAgent, (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform ?? '', navigator.maxTouchPoints);
  fetch('/api/downloads')
    .then(async (r) => {
      if (!r.ok || !(r.headers.get('content-type') ?? '').includes('json')) return (error = UNAVAILABLE);
      manifest = await r.json();
      if (!choices(manifest!, mine).length) error = UNAVAILABLE;
    })
    .catch(() => (error = "Couldn't reach CaveCAD. Check your connection."));
  const list = $derived(manifest ? choices(manifest, mine) : []);
</script>

<svelte:window onkeydown={(e) => e.key === 'Escape' && onclose()} />
<div class="back" role="presentation" onclick={onclose}></div>
<div class="panel" role="dialog" aria-modal="true" aria-label="Get the desktop app">
  <h2>Get the desktop app</h2>
  {#if manifest}<p class="hint">Version {manifest.version}. Works offline and opens cave folders on your computer.</p>{/if}
  {#if error}<p class="hint bad">{error}</p>{/if}
  {#each list as c, i}
    <div class="item">
      <a class="btn" class:primary={i === 0} href={'/downloads/' + encodeURIComponent(c.file)} download>{c.label}</a>
      <p class="hint">{c.steps}</p>
    </div>
  {/each}
  <div class="row"><button class="btn" onclick={onclose}>Close</button></div>
</div>

<style>
  .back { position: fixed; inset: 0; background: rgb(0 0 0 / 0.3); z-index: 90; }
  .panel { position: fixed; z-index: 91; left: 50%; top: 50%; transform: translate(-50%, -50%); width: min(460px, calc(100% - 32px)); max-height: calc(100% - 32px); overflow: auto; display: flex; flex-direction: column; gap: 10px; padding: 20px; background: var(--panel); border: 1px solid var(--line); border-radius: 12px; color: var(--text); }
  h2 { margin: 0; font-size: 18px; color: var(--text-strong); }
  .item { display: flex; flex-direction: column; gap: 4px; }
  .btn { display: block; text-align: center; padding: 8px 12px; background: var(--panel-2); color: var(--text); border: 1px solid var(--line); border-radius: var(--radius); text-decoration: none; cursor: pointer; font: inherit; }
  .btn.primary { background: var(--accent); color: #fff; border-color: var(--accent); }
  .hint { margin: 0; color: var(--muted); font-size: 12px; }
  .bad { color: var(--danger); }
  .row { display: flex; justify-content: flex-end; }
</style>
