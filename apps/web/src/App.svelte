<script lang="ts">
  import { onMount } from 'svelte';
  import Ribbon from './components/Ribbon.svelte';
  import NewCave from './components/NewCave.svelte';
  import { initWorkspace } from './lib/workspace.svelte';
  import { engine, loadEngine, loadBuildInfo, type BuildInfo } from './lib/engine';
  import { unsaved } from './lib/cave.svelte';
  import { guardUnload } from './lib/files';
  import { installDiagnostics } from './lib/diagnostics';
  import { readInviteFromUrl } from './lib/sync.svelte';
  import { checkRemembered } from './lib/scans.svelte';

  let version = $state('loading engine…');
  let info = $state<BuildInfo | null>(null);
  let failed = $state<string | null>(null);
  let host: HTMLDivElement;

  onMount(() => {
    const teardown = initWorkspace(host);
    installDiagnostics();
    readInviteFromUrl();
    void checkRemembered(); // the scans kept from last time come back before any panel is opened
    const unguard = guardUnload();
    (async () => {
      try {
        await loadEngine();
        version = engine.version();
      } catch (e) {
        failed = String(e);
      }
      info = await loadBuildInfo();
    })();
    return () => {
      unguard();
      teardown();
    };
  });

  const dirty = $derived(unsaved());
  $effect(() => {
    document.title = `${dirty ? '• ' : ''}CaveCAD`; // a dot in the tab says there is something unsaved
  });

  const built = $derived(info?.builtAt ? new Date(info.builtAt).toLocaleTimeString() : '');
</script>

<div class="shell">
  <Ribbon />
  <NewCave />
  {#if info?.error}
    <div class="banner">Rust build failed. The page is showing the last good build.<pre>{info.error}</pre></div>
  {/if}
  {#if failed}<p class="err">Engine failed to load: {failed}</p>{/if}
  <div class="workspace" bind:this={host}></div>
  <footer>
    <span>{version}</span>
    {#if dirty}<span class="unsaved" title="Changes since the cave was opened or last saved. Save with Ctrl/Cmd+S.">Unsaved changes</span>{/if}
    {#if info}<span>build {info.git}{info.dirty ? '+' : ''} · {built}</span>{/if}
  </footer>
</div>

<style>
  .shell { display: flex; flex-direction: column; height: 100%; }
  .workspace { flex: 1; min-height: 0; border-top: 1px solid var(--line); }
  .err, .banner { color: var(--danger); margin: 0; }
  .banner { background: var(--accent-bg); padding: 8px 12px; }
  .banner pre { margin: 4px 0 0; font-family: var(--mono); font-size: 12px; white-space: pre-wrap; }
  .unsaved { color: var(--warn); font-weight: 600; }
  footer { display: flex; gap: 16px; padding: 4px 12px; font-size: 12px; color: var(--muted);
    border-top: 1px solid var(--line); background: var(--panel); }
</style>
