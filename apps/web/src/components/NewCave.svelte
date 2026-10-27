<script lang="ts">
  // Starting a cave from nothing: its name, the unit the notes are written in, and where its project
  // folder goes (made with its standard subfolders; the cave file is written into it). Everything else
  // is built up from the scans, shot by shot, in the Notebook.
  import { cave, setUnits } from '../lib/cave.svelte';
  import { engine } from '../lib/engine';
  import { confirmDiscard, openFromHome, startCave } from '../lib/files';
  import { workLayout } from '../lib/workspace.svelte';
  import { shelf } from '../lib/shelf.svelte';
  import { canMakeFolders, choosePlace, folderNameOf, look, make, startPlace, type Place } from '../lib/cavefolder.svelte';
  import { isDesktop } from '../lib/desktop';
  import { isCaveFolder } from '../lib/folder-handle';

  let name = $state('');
  let unit = $state<'ft' | 'm'>(cave.units === 'imperial' ? 'ft' : 'm');
  let message = $state('');
  let input = $state<HTMLInputElement>();
  const folders = canMakeFolders();
  let place = $state.raw<Place | null>(null);
  let busy = $state(false);
  /** What is already where the folder would go, waiting for the person to choose. */
  let found = $state<{ kind: 'hasCave'; file: string; label: string } | { kind: 'notACave'; label: string } | null>(null);

  $effect(() => {
    if (!cave.newDialog) return;
    queueMicrotask(() => input?.focus());
    if (folders && !place) void startPlace().then((p) => (place ??= p)).catch(() => undefined);
  });

  const typed = () => name.trim() || 'New cave';
  /** A browser starts with nothing picked: Chrome will not give a page Documents itself, only a folder inside it. */
  const browser = !isDesktop();
  const where = $derived(
    !place
      ? browser ? 'Not chosen yet' : `Documents/${folderNameOf(typed())}`
      : place.kind === 'handle' && isCaveFolder(place.handle, folderNameOf(typed()))
        ? place.label
        : `${place.label.replace(/[\\/]+$/, '')}/${folderNameOf(typed())}`,
  );

  async function choose() {
    found = null;
    message = '';
    try {
      place = (await choosePlace(place)) ?? place;
    } catch (e) {
      message = `That place could not be used: ${e instanceof Error ? e.message : String(e)}`;
    }
  }

  function done(r: { ok: boolean; message: string } | null, layout: boolean) {
    busy = false;
    if (r === null) return; // declined to leave the cave on screen
    engine.log(r.ok ? 'info' : 'error', 'file', r.message);
    if (!r.ok) {
      message = r.message;
      return;
    }
    // What people read and type follows what the notes are written in, so numbers go in as written.
    if (layout) setUnits(unit === 'ft' ? 'imperial' : 'metric');
    cave.newDialog = false;
    shelf.open = false;
    name = '';
    found = null;
    if (layout) workLayout(); // scans and notebook side by side, ready to start typing
  }

  async function create(useExisting = false) {
    message = '';
    if (!folders) return done(startCave(name, unit), true);
    let at = place;
    if (!at) {
      // a browser names no place until one is picked; its picker starts in Documents
      at = await choosePlace(null).catch(() => null);
      if (!at) return;
      place = at;
    }
    busy = true;
    try {
      const there = await look(at, typed());
      if (there.place.kind === 'hasCave') {
        found = { kind: 'hasCave', file: there.place.file, label: there.label };
        busy = false;
        return;
      }
      if (there.place.kind === 'notACave' && !useExisting) {
        found = { kind: 'notACave', label: there.label };
        busy = false;
        return;
      }
      if (!confirmDiscard()) {
        busy = false;
        return;
      }
      const home = await make(at, typed());
      done(startCave(name, unit, home, true), true);
    } catch (e) {
      busy = false;
      message = `The cave's folder could not be made: ${e instanceof Error ? e.message : String(e)}`;
    }
  }

  async function openFound() {
    if (!place || found?.kind !== 'hasCave') return;
    if (!confirmDiscard()) return;
    busy = true;
    try {
      const home = await make(place, typed(), found.file); // adds only the subfolders it is missing
      done(await openFromHome(home, true), false);
    } catch (e) {
      busy = false;
      message = `That cave could not be opened: ${e instanceof Error ? e.message : String(e)}`;
    }
  }

  const close = () => {
    found = null;
    cave.newDialog = false;
  };
</script>

{#if cave.newDialog}
  <div class="veil" role="presentation" onclick={close}>
    <div class="box" role="dialog" aria-modal="true" aria-label="New cave" tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === 'Escape' && close()}>
      <h2>New cave</h2>
      <label>Name
        <input bind:this={input} bind:value={name} placeholder="Cave name" maxlength="80" oninput={() => (found = null)} onkeydown={(e) => e.key === 'Enter' && !found && create()} />
      </label>
      <fieldset>
        <legend>The notes are written in</legend>
        <label><input type="radio" bind:group={unit} value="ft" /> Feet</label>
        <label><input type="radio" bind:group={unit} value="m" /> Metres</label>
      </fieldset>
      {#if folders}
        <div class="field">
          <span class="lbl">Folder</span>
          <div class="where">
            <span class="path" title={where}><bdi>{where}</bdi></span>
            <button onclick={choose} disabled={busy}>Choose…</button>
          </div>
          <span class="small">A folder named after the cave, with scans, survey, images, lidar, surface, documents, trips, exports and backup inside. The cave file goes in it.</span>
          {#if browser}<span class="small">The browser can't use Documents itself: choose a folder inside it, such as Documents/Caves, or make one there with New Folder. A folder you make with the cave's name is used as the cave's folder.</span>{/if}
        </div>
      {:else}
        <p class="hint">This cave is kept in this browser. Export → Project gives you a folder of it; Chrome, Edge and the desktop app can make the folder for you.</p>
      {/if}
      {#if found?.kind === 'hasCave'}
        <div class="found" role="alert">
          <p>{found.label} already holds a cave ({found.file}).</p>
          <div class="row"><button onclick={choose}>Choose another place</button><button class="go" onclick={openFound} disabled={busy}>Open that cave</button></div>
        </div>
      {:else if found?.kind === 'notACave'}
        <div class="found" role="alert">
          <p>{found.label} already exists. Use it for this cave? Nothing in it is changed; only the missing folders are added.</p>
          <div class="row"><button onclick={choose}>Choose another place</button><button class="go" onclick={() => create(true)} disabled={busy}>Use it</button></div>
        </div>
      {/if}
      <p class="hint">You will get your scans on the left and the notebook beside them. Pick the first page and type its shots as you read them.</p>
      {#if message}<p class="err" role="alert">{message}</p>{/if}
      <div class="row">
        <button onclick={close}>Cancel</button>
        <button class="go" onclick={() => create()} disabled={busy || found !== null}>Start the cave</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .veil { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.5); display: grid; place-items: center; z-index: 50; }
  .box { width: min(460px, 92vw); background: var(--panel); color: var(--text); border: 1px solid var(--line); border-radius: 10px; padding: 18px 20px; display: flex; flex-direction: column; gap: 12px; outline: none; }
  h2 { margin: 0; font-size: 17px; }
  label { display: flex; flex-direction: column; gap: 4px; font-size: 12px; color: var(--muted); }
  fieldset { border: 1px solid var(--line); border-radius: 8px; display: flex; gap: 16px; padding: 6px 10px 8px; font-size: 13px; }
  fieldset label { flex-direction: row; align-items: center; gap: 6px; color: var(--text); }
  legend { font-size: 12px; color: var(--muted); padding: 0 4px; }
  input:not([type='radio']) { font: inherit; font-size: 14px; padding: 6px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); }
  .hint { margin: 0; font-size: 12px; color: var(--muted); }
  .field { display: flex; flex-direction: column; gap: 4px; }
  .lbl { font-size: 12px; color: var(--muted); }
  .where { display: flex; gap: 8px; align-items: center; }
  /* a long path shows its end (the cave's own folder), not its start */
  .path { flex: 1; min-width: 0; font-size: 13px; padding: 6px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; direction: rtl; text-align: left; }
  .small { font-size: 11px; color: var(--muted); }
  .found { border: 1px solid var(--accent); border-radius: 8px; padding: 8px 10px; display: flex; flex-direction: column; gap: 8px; font-size: 13px; }
  .found p { margin: 0; }
  .err { margin: 0; color: var(--danger); font-size: 12px; }
  .row { display: flex; justify-content: flex-end; gap: 8px; }
  button { font: inherit; font-size: 13px; padding: 6px 14px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); cursor: pointer; }
  button:disabled { opacity: 0.5; cursor: default; }
  button.go { background: var(--accent); border-color: var(--accent); color: #fff; }
</style>
