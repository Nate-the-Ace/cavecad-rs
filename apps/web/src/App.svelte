<script lang="ts">
  import { selectAllInLastView } from './lib/select-all';
  import SnapMenu from './components/SnapMenu.svelte';
  import { copyEvent, deleteSelection, pasteEvent } from './lib/view-clip.svelte';
  import LineMenu from './components/LineMenu.svelte';
  import TextEditor from './components/TextEditor.svelte';
  import { onMount, untrack } from 'svelte';
  import Ribbon from './components/Ribbon.svelte';
  import NewCave from './components/NewCave.svelte';
  import SurfaceData from './components/SurfaceData.svelte';
  import { recomputeLayers, resetEntrancePick, surface, surfaceKey, syncSurface } from './lib/surface.svelte';
  import { initWorkspace } from './lib/workspace.svelte';
  import { engine, loadEngine, loadBuildInfo, type BuildInfo } from './lib/engine';
  import { cave, unsaved } from './lib/cave.svelte';
  import { guardUnload } from './lib/files';
  import { installDiagnostics } from './lib/diagnostics';
  import CaveShelf from './components/CaveShelf.svelte';
  import MovedPage from './components/MovedPage.svelte';
  import { isOldJoinUrl, isOldLiveLink } from './lib/moved-link';
  import { listenForLinks } from './lib/desktop';
  import { syncDrawing } from './lib/drawing';
  import { autosaveSoon, flush, loadShelf, shelf } from './lib/shelf.svelte';
  import { checkRemembered } from './lib/scans.svelte';
  import AccountGate from './components/account/AccountGate.svelte';
  import { account, handleAuthLink, startAccount } from './lib/account.svelte';
  import { startRooms } from './lib/room.svelte';

  let version = $state('loading engine…');
  let info = $state<BuildInfo | null>(null);
  let failed = $state<string | null>(null);
  let host: HTMLDivElement;
  /** Opened from an old live, class or join link: say where those went (MovedPage.svelte). */
  let moved = $state(isOldLiveLink(location.search));

  // The drawing's scale (and who changed it) is read again whenever the cave changes or another opens.
  let engineReady = $state(false);
  $effect(() => {
    void cave.version;
    void cave.opened;
    if (engineReady) untrack(syncDrawing);
  });

  // The surface files follow the cave's surface record: a cave opened or switched, or an undo/redo that
  // changes the record, loads them again (and downloads any this device does not have).
  let lastSurface = '';
  let lastPickCave = cave.opened;
  $effect(() => {
    void cave.version;
    void cave.opened;
    if (!engineReady) return;
    untrack(() => {
      const key = `${cave.opened}|${surfaceKey()}`;
      if (key === lastSurface) return;
      lastSurface = key;
      // Recording the key here is safe even while a download runs: syncSurface then waits and runs
      // again when that download ends (its pending flag), so this cave's files are still fetched.
      void syncSurface();
    });
  });

  // Pick mode belongs to one cave's ground: another cave opened, or the elevation gone, ends it.
  $effect(() => {
    void cave.opened;
    void surface.tiff;
    untrack(() => {
      if (!surface.tiff || lastPickCave !== cave.opened) resetEntrancePick();
      lastPickCave = cave.opened;
    });
  });

  // What the views draw of the surface (photo place, contours, terrain) is rebuilt from the kept files when
  // they change, the cave changes, or the unit or Adjusted setting does: never a download. Edits that come
  // in a burst are rebuilt once, on the next frame.
  let layersFrame = 0;
  $effect(() => {
    void surface.version;
    void cave.version;
    void cave.opened;
    void cave.units;
    void cave.adjusted;
    if (!engineReady) return;
    untrack(() => {
      if (layersFrame) return;
      layersFrame = requestAnimationFrame(() => {
        layersFrame = 0;
        recomputeLayers();
      });
    });
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
    void startAccount();
    const stopRooms = startRooms(); // the open cave kept in step through its Private room
    window.addEventListener('keydown', noPageSelectAll);
    window.addEventListener('copy', onCopy);
    window.addEventListener('cut', onCut);
    window.addEventListener('paste', onPaste);
    const teardown = initWorkspace(host);
    installDiagnostics();
    listenForLinks((url) => {
      void handleAuthLink(url).then((signIn) => { if (!signIn && isOldJoinUrl(url)) moved = true; });
    }); // in the desktop app, a link that opened it
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
        // A clean slate: the shelf opens first (an old live link opens it from the moved page instead).
        if (!cave.hasCave && shelf.startup && !moved) shelf.open = true;
      } catch (e) {
        failed = String(e);
      }
      info = await loadBuildInfo();
    })();
    return () => {
      stopRooms();
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
    autosaveSoon();
  });
  // A multiple selection belongs to the line that was picked first: picking another line on its own drops the rest.
  $effect(() => {
    if (cave.alsoOf !== cave.selected && (cave.also.length || cave.alsoOf)) {
      cave.also = [];
      cave.alsoOf = '';
    }
  });

  const keepNow = () => void flush();

  const dirty = $derived(unsaved());
  $effect(() => {
    document.title = `${dirty ? '• ' : ''}CaveCAD`; // a dot in the tab says there is something unsaved
  });

  // While the sign-in gate covers the page, the app behind it is inert, and the keyboard shortcuts and
  // clipboard handlers listening on the window don't hear keys meant for (or outside) the gate.
  const open = $derived(account.phase === 'ready' || account.phase === 'offline' || account.phase === 'offline-expired');
  $effect(() => {
    if (open) return;
    const TYPES = ['keydown', 'keyup', 'keypress', 'copy', 'cut', 'paste'];
    const stop = (e: Event) => {
      if ((e.target as Element | null)?.closest?.('[data-account-gate]')) return;
      e.stopImmediatePropagation();
    };
    for (const t of TYPES) window.addEventListener(t, stop, { capture: true });
    return () => {
      for (const t of TYPES) window.removeEventListener(t, stop, { capture: true });
    };
  });

  const built = $derived(info?.builtAt ? new Date(info.builtAt).toLocaleTimeString() : '');
</script>

<div class="shell" inert={!open}>
  <Ribbon />
  <SnapMenu />
  <NewCave />
  <SurfaceData />
  <CaveShelf />
  <MovedPage bind:open={moved} />
  <LineMenu />
  <TextEditor />
  {#if info?.error}
    <div class="banner">Rust build failed. The page is showing the last good build.<pre>{info.error}</pre></div>
  {/if}
  {#if failed}<p class="err">Engine failed to load: {failed}</p>{/if}
  <div class="body">
    <div class="workspace" bind:this={host}></div>
  </div>
  <footer>
    <span>{version}</span>
    {#if dirty}<span class="unsaved" title="Changes since the cave was opened or last saved. Save with Ctrl/Cmd+S.">Unsaved changes</span>{/if}
    {#if info}<span>build {info.git}{info.dirty ? '+' : ''} · {built}</span>{/if}
  </footer>
</div>
<AccountGate />

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
