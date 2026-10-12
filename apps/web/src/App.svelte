<script lang="ts">
  import { onMount } from 'svelte';
  import Ribbon from './components/Ribbon.svelte';
  import Console from './components/Console.svelte';
  import { engine, loadEngine, loadBuildInfo, type BuildInfo } from './lib/engine';

  let version = $state('loading engine…');
  let info = $state<BuildInfo | null>(null);
  let failed = $state<string | null>(null);

  onMount(async () => {
    try {
      await loadEngine();
      version = engine.version();
    } catch (e) {
      failed = String(e);
    }
    info = await loadBuildInfo();
  });

  const built = $derived(info?.builtAt ? new Date(info.builtAt).toLocaleTimeString() : '');
</script>

<div class="shell">
  <Ribbon />
  {#if info?.error}
    <div class="banner">Rust build failed. The page is showing the last good build.<pre>{info.error}</pre></div>
  {/if}
  <main class="canvas">
    {#if failed}<p class="err">Engine failed to load: {failed}</p>{:else}<p class="hint">Map canvas goes here.</p>{/if}
  </main>
  <div class="dock"><Console /></div>
  <footer>
    <span>{version}</span>
    {#if info}<span>build {info.git}{info.dirty ? '+' : ''} · {built}</span>{/if}
  </footer>
</div>

<style>
  .shell { display: flex; flex-direction: column; height: 100%; }
  .dock { height: 220px; display: grid; }
  .canvas { flex: 1; min-height: 0; display: grid; place-items: center; background: var(--panel-2); border-top: 1px solid var(--line); }
  .hint { color: var(--muted); }
  .err, .banner { color: var(--danger); }
  .banner { background: var(--accent-bg); padding: 8px 12px; }
  .banner pre { margin: 4px 0 0; font-family: var(--mono); font-size: 12px; white-space: pre-wrap; }
  footer { display: flex; gap: 16px; padding: 4px 12px; font-size: 12px; color: var(--muted);
    border-top: 1px solid var(--line); background: var(--panel); }
</style>
