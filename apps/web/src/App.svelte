<script lang="ts">
  import { selectAllInLastView } from './lib/select-all';
  import { copyEvent, deleteSelection, pasteEvent } from './lib/view-clip.svelte';
  import LineMenu from './components/LineMenu.svelte';
  import { onMount, untrack } from 'svelte';
  import Ribbon from './components/Ribbon.svelte';
  import NewCave from './components/NewCave.svelte';
  import { initWorkspace } from './lib/workspace.svelte';
  import { engine, loadEngine, loadBuildInfo, type BuildInfo } from './lib/engine';
  import { cave, unsaved } from './lib/cave.svelte';
  import { guardUnload } from './lib/files';
  import { installDiagnostics } from './lib/diagnostics';
  import { live, readInviteFromUrl } from './lib/sync.svelte';
  import CaveShelf from './components/CaveShelf.svelte';
  import LiveShare from './components/LiveShare.svelte';
  import ClassroomDock from './components/ClassroomDock.svelte';
  import ClassroomDialogs from './components/ClassroomDialogs.svelte';
  import ClassroomAlerts from './components/ClassroomAlerts.svelte';
  import { classroom, keepsCave, openJoinUrl, readClassLinkFromUrl } from './lib/classroom.svelte';
  import { listenForLinks } from './lib/desktop';
  import { syncDrawing } from './lib/drawing';
  import { viewChanged } from './lib/p2p.svelte';
  import { scans } from './lib/scans.svelte';
  import { autosaveSoon, flush, loadShelf, shelf } from './lib/shelf.svelte';
  import { checkRemembered } from './lib/scans.svelte';

  let version = $state('loading engine…');
  let info = $state<BuildInfo | null>(null);
  let failed = $state<string | null>(null);
  let host: HTMLDivElement;

  // The drawing's scale (and who changed it) is read again whenever the cave changes or another opens.
  let engineReady = $state(false);
  $effect(() => {
    void cave.version;
    void cave.opened;
    if (engineReady) untrack(syncDrawing);
  });

  // Select All outside a text field must not select the page's own buttons and labels.
  function noPageSelectAll(e: KeyboardEvent) {
    if (!(e.metaKey || e.ctrlKey) || e.altKey || e.key.toLowerCase() !== 'a') return;
    const t = e.target as HTMLElement | null;
    if (t && (t.isContentEditable || /^(input|textarea|select)$/i.test(t.tagName))) return;
    e.preventDefault();
    if (!cave.tool) selectAllInLastView();
  }

  /** True when the keyboard is in a text field, where copy and paste are about text. */
  const inField = (e: Event) => {
    const t = e.target as HTMLElement | null;
    return !!t && (t.isContentEditable || /^(input|textarea|select)$/i.test(t.tagName));
  };
  const onCopy = (e: ClipboardEvent) => {
    if (!inField(e)) copyEvent(e);
  };
  const onCut = (e: ClipboardEvent) => {
    if (!inField(e) && copyEvent(e)) deleteSelection();
  };
  const onPaste = (e: ClipboardEvent) => {
    if (!inField(e)) pasteEvent(e);
  };

  onMount(() => {
    window.addEventListener('keydown', noPageSelectAll);
    window.addEventListener('copy', onCopy);
    window.addEventListener('cut', onCut);
    window.addEventListener('paste', onPaste);
    const teardown = initWorkspace(host);
    installDiagnostics();
    readInviteFromUrl();
    readClassLinkFromUrl();
    listenForLinks(openJoinUrl); // in the desktop app, a link that opened it
    void checkRemembered(); // the scans kept from last time come back before any panel is opened
    const unguard = guardUnload();
    document.addEventListener('visibilitychange', keepNow);
    window.addEventListener('pagehide', keepNow);
    (async () => {
      try {
        await loadEngine();
        version = engine.version();
        engineReady = true;
        syncDrawing();
        await loadShelf();
        // A clean slate: the shelf opens first, unless someone sent an invite to a room.
        if (!cave.hasCave && shelf.startup && !live.pending && !classroom.pending) shelf.open = true;
      } catch (e) {
        failed = String(e);
      }
      info = await loadBuildInfo();
    })();
    return () => {
      unguard();
      document.removeEventListener('visibilitychange', keepNow);
      window.removeEventListener('pagehide', keepNow);
      window.removeEventListener('keydown', noPageSelectAll);
      window.removeEventListener('copy', onCopy);
      window.removeEventListener('cut', onCut);
      window.removeEventListener('paste', onPaste);
      teardown();
    };
  });

  // Edits are kept on the shelf a moment after they are made.
  $effect(() => {
    void cave.version;
    if (keepsCave()) autosaveSoon(); // what a student is shown is the teacher's cave: not kept here
  });
  // The host tells guests where it is looking.
  $effect(() => {
    void cave.trip;
    void cave.run;
    void cave.station;
    void scans.selected;
    viewChanged();
  });
  // A multiple selection belongs to the line that was picked first: picking another line on its own drops the rest.
  $effect(() => {
    if (cave.alsoOf !== cave.selected && (cave.also.length || cave.alsoOf)) {
      cave.also = [];
      cave.alsoOf = '';
    }
  });

  const keepNow = () => keepsCave() && void flush();

  const dirty = $derived(unsaved());
  $effect(() => {
    document.title = `${dirty ? '• ' : ''}CaveCAD`; // a dot in the tab says there is something unsaved
  });

  const built = $derived(info?.builtAt ? new Date(info.builtAt).toLocaleTimeString() : '');
</script>

<div class="shell">
  <Ribbon />
  <NewCave />
  <CaveShelf />
  <LiveShare />
  <ClassroomDialogs />
  <ClassroomAlerts />
  <LineMenu />
  {#if info?.error}
    <div class="banner">Rust build failed. The page is showing the last good build.<pre>{info.error}</pre></div>
  {/if}
  {#if failed}<p class="err">Engine failed to load: {failed}</p>{/if}
  <div class="body">
    <div class="workspace" bind:this={host}></div>
    <ClassroomDock />
  </div>
  <footer>
    <span>{version}</span>
    {#if dirty}<span class="unsaved" title="Changes since the cave was opened or last saved. Save with Ctrl/Cmd+S.">Unsaved changes</span>{/if}
    {#if info}<span>build {info.git}{info.dirty ? '+' : ''} · {built}</span>{/if}
  </footer>
</div>

<style>
  .shell { display: flex; flex-direction: column; height: 100%; }
  .body { flex: 1; min-height: 0; display: flex; border-top: 1px solid var(--line); }
  .workspace { flex: 1; min-width: 0; min-height: 0; }
  .err, .banner { color: var(--danger); margin: 0; }
  .banner { background: var(--accent-bg); padding: 8px 12px; }
  .banner pre { margin: 4px 0 0; font-family: var(--mono); font-size: 12px; white-space: pre-wrap; }
  .unsaved { color: var(--warn); font-weight: 600; }
  footer { display: flex; gap: 16px; padding: 4px 12px; font-size: 12px; color: var(--muted);
    border-top: 1px solid var(--line); background: var(--panel); }
</style>
