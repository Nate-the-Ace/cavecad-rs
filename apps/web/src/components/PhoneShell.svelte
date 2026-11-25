<script lang="ts">
  // The phone layout (an iPhone, either way up): one row of pill tabs along the top (Caves, Survey, Sketch, Notebook,
  // Settings) and one screen below it. There is no other header: the Caves pill carries the open cave's name and the
  // unsaved dot, the Survey pill a dot for the instrument, and Surface Data sits in the Sketch bar.
  // Gates come first (lib/gates.ts): until a cave is open only the Caves tab works; opening one shows Survey.
  // Each screen is made the first time it is opened and then kept (hidden, not destroyed), so switching back finds it
  // as it was, the Map's 3D view included. The ribbon is not shown: what the survey needs from it is in the Survey tab.
  import { untrack } from 'svelte';
  import { CloudOff, Settings as SettingsIcon } from '@lucide/svelte';
  import '../phone.css';
  import CaveShelf from './CaveShelf.svelte';
  import PhoneSketch from './PhoneSketch.svelte';
  import PhoneNotebook from './PhoneNotebook.svelte';
  import PhoneSettings from './PhoneSettings.svelte';
  import SurveyPanel from '../panels/SurveyPanel.svelte';
  import { account } from '../lib/account.svelte';
  import { cave, unsaved } from '../lib/cave.svelte';
  import { shelf } from '../lib/shelf.svelte';
  import { survey } from '../lib/survey.svelte';
  import { currentGate, signedInPhase } from '../lib/gates';
  import { PHONE_TABS, phone, showPhoneTab, type PhoneTab } from '../lib/phone.svelte';

  const LABELS: Record<PhoneTab, string> = { caves: 'Caves', survey: 'Survey', sketch: 'Sketch', notebook: 'Notebook', settings: 'Settings' };

  // The open cave, as the gate sees it. Removing the open cave from the shelf ("forgetting" it) counts as none open,
  // until another cave is opened.
  let forgotten = $state(false);
  let lastId = shelf.currentId;
  $effect(() => {
    const id = shelf.currentId;
    const records = shelf.records;
    untrack(() => {
      if (lastId && !id && !records.some((r) => r.id === lastId)) forgotten = true;
      lastId = id;
    });
  });
  let lastOpened = cave.opened;
  $effect(() => {
    const o = cave.opened;
    untrack(() => {
      if (o === lastOpened) return;
      lastOpened = o;
      forgotten = false;
      if (cave.hasCave && phone.tab === 'caves') showPhoneTab('survey'); // another cave picked on the Caves tab: on to the survey
    });
  });

  const gate = $derived(currentGate({ signedIn: signedInPhase(account.phase), caveOpen: cave.hasCave && !forgotten }));
  /** The tab on screen: Caves while a gate is up, whatever was chosen before. */
  const tab = $derived<PhoneTab>(gate ? 'caves' : phone.tab);

  // The gates lifting (a cave picked, or signed in again with one open) shows the Survey tab.
  let wasGated = false;
  $effect(() => {
    const up = !!gate;
    untrack(() => {
      if (wasGated && !up) showPhoneTab('survey');
      wasGated = up;
    });
  });

  // Anything that asks for the Cave Shelf window (the Map's Caves button, an old link) gets the Caves tab instead.
  $effect(() => {
    if (!shelf.open) return;
    untrack(() => {
      shelf.open = false;
      showPhoneTab('caves');
    });
  });

  // made once opened, then kept (Caves is drawn only while showing: it reads the server's list while it is up)
  let made = $state<Record<string, boolean>>({});
  $effect(() => {
    if (!made[tab]) made = { ...made, [tab]: true };
  });

  const linked = $derived(survey.status === 'connected');
  const dirty = $derived(unsaved());
  const caveLabel = $derived(cave.hasCave && !forgotten ? cave.name || 'Untitled cave' : 'Caves');

  // keep the chosen pill in view when the row scrolls
  let row = $state<HTMLElement | null>(null);
  $effect(() => {
    void tab;
    row?.querySelector('[aria-selected="true"]')?.scrollIntoView?.({ block: 'nearest', inline: 'nearest' });
  });
</script>

<div class="phone shell-phone">
  <div class="tabs" role="tablist" aria-label="Screens" bind:this={row}>
    {#each PHONE_TABS as id (id)}
      {@const locked = !!gate && id !== 'caves'}
      <button
        role="tab"
        class="tab {id}"
        aria-selected={tab === id}
        aria-label={id === 'settings' ? 'Settings' : id === 'survey' ? `Survey: ${linked ? survey.device || 'instrument connected' : 'no instrument'}` : id === 'caves' ? `Caves: ${caveLabel}${dirty ? ', unsaved changes' : ''}` : undefined}
        disabled={locked}
        onclick={() => showPhoneTab(id)}
      >
        <span class="pill">
          {#if id === 'settings'}<SettingsIcon size={18} />
          {:else if id === 'caves'}<span class="cname">{caveLabel}</span>{#if dirty && cave.hasCave}<span class="unsaved" title="Unsaved changes"></span>{/if}
          {:else}{LABELS[id]}{#if id === 'survey'}<span class="link" class:on={linked} title={linked ? survey.device || 'Connected' : 'No instrument'}></span>{/if}{/if}
        </span>
      </button>
    {/each}
    {#if account.phase === 'offline' || account.phase === 'offline-expired'}
      <span class="offline" class:expired={account.phase === 'offline-expired'} role="status" aria-label={account.phase === 'offline' ? "Offline: working on what's on this device" : 'Offline over 30 days'} title="Offline"><CloudOff size={18} /></span>
    {/if}
  </div>

  <main class="panes">
    {#if tab === 'caves'}
      <section class="pane caves" aria-label="Caves">
        {#if gate?.reason === 'cave'}
          <div class="gate" role="status">
            <span>{gate.message}</span>
          </div>
        {/if}
        <div class="shelf"><CaveShelf embedded /></div>
      </section>
    {/if}
    {#if made.survey}<section class="pane" aria-label="Survey" hidden={tab !== 'survey'}><SurveyPanel /></section>{/if}
    {#if made.sketch}<section class="pane" aria-label="Sketch" hidden={tab !== 'sketch'}><PhoneSketch /></section>{/if}
    {#if made.notebook}<section class="pane" aria-label="Notebook" hidden={tab !== 'notebook'}><PhoneNotebook /></section>{/if}
    {#if made.settings}<section class="pane" aria-label="Settings" hidden={tab !== 'settings'}><PhoneSettings /></section>{/if}
  </main>
</div>

<style>
  .phone {
    display: flex; flex-direction: column; flex: 1; width: 100%; min-width: 0; height: 100%; background: var(--bg, var(--panel));
    padding: 0 env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left); box-sizing: border-box;
  }
  /* one slim row: 44px hit areas, a 34px pill inside each */
  .tabs {
    display: flex; gap: 0; flex: none; padding: env(safe-area-inset-top) 2px 0; overflow-x: auto; overflow-y: hidden;
    scrollbar-width: none; touch-action: pan-x; background: var(--panel); border-bottom: 1px solid var(--line);
  }
  .tabs::-webkit-scrollbar { display: none; }
  .tab { flex: none; min-height: var(--p-hit); min-width: var(--p-hit); padding: 0 1px; border: 0; background: none; color: var(--text); font: inherit; display: flex; align-items: center; touch-action: manipulation; cursor: pointer; }
  .tab:disabled { opacity: 0.35; cursor: default; }
  .pill { display: inline-flex; align-items: center; justify-content: center; gap: 5px; height: 34px; min-width: 34px; padding: 0 9px; border-radius: 999px; background: var(--panel-2); font-size: var(--p-font); font-weight: 500; white-space: nowrap; box-sizing: border-box; }
  .tab.settings .pill { padding: 0 8px; }
  .tab[aria-selected='true'] .pill { background: var(--accent); color: #fff; }
  .cname { max-width: 84px; overflow: hidden; text-overflow: ellipsis; }
  .unsaved { width: 7px; height: 7px; border-radius: 50%; background: var(--warn); flex: none; }
  .link { width: 7px; height: 7px; border-radius: 50%; border: 1.5px solid currentColor; opacity: 0.5; flex: none; box-sizing: border-box; }
  .link.on { background: var(--ok, #2fb36d); border-color: var(--ok, #2fb36d); opacity: 1; }
  .offline { flex: none; display: inline-flex; align-items: center; justify-content: center; min-width: var(--p-hit); min-height: var(--p-hit); color: var(--muted); }
  .offline.expired { color: var(--danger); }
  .panes { flex: 1; min-height: 0; position: relative; }
  .pane { position: absolute; inset: 0; overflow: hidden; }
  .pane[hidden] { display: none; }
  .pane.caves { display: flex; flex-direction: column; }
  .gate { display: flex; align-items: center; gap: var(--p-gap); padding: var(--p-gap) var(--p-pad); background: var(--accent-bg); border-bottom: 1px solid var(--accent); font-weight: 600; font-size: var(--p-font-small); }
  .gate span { flex: 1; min-width: 0; }
  .shelf { flex: 1; min-height: 0; display: flex; }
</style>
