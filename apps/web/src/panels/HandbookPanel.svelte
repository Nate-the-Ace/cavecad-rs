<script lang="ts">
  // The Handbook, read like a wiki: contents and search beside the page when the panel is wide, one pane at a
  // time when it is narrow. Pages ship with the app (spec 2026-11-09-handbook-design.md); nothing here edits them.
  import { catalog, shotUrl } from '../handbook/bundle';
  import { renderMarkdown } from '../handbook/markdown';
  import { goBack, goForward, goTo, handbook } from '../lib/handbook.svelte';

  let width = $state(0);
  let query = $state('');
  let showList = $state(false);
  const wide = $derived(width >= 640);
  const page = $derived(catalog.byId(handbook.page) ?? catalog.pages[0]);
  const html = $derived(
    page ? renderMarkdown(page.text, (kind, t) => (kind === 'link' ? (catalog.byId(t)?.text ? `#hb:${t}` : null) : shotUrl(t))) : '',
  );
  const found = $derived(catalog.search(query));

  function open(id: string) {
    goTo(id);
    showList = false;
    query = '';
  }

  function onClick(e: MouseEvent) {
    const a = (e.target as HTMLElement).closest('a[data-page]');
    if (!a) return;
    e.preventDefault();
    open(a.getAttribute('data-page') ?? '');
  }
</script>

<div class="hb" class:wide bind:clientWidth={width}>
  {#if wide || showList}
    <nav class="side">
      <input type="search" placeholder="Search the Handbook" bind:value={query} />
      {#if query.trim()}
        {#each found as p (p.id)}<button class="item" onclick={() => open(p.id)}>{p.title}</button>{:else}<p class="none">Nothing found.</p>{/each}
      {:else}
        {#each catalog.groups() as g (g.name)}
          <h4>{g.name}</h4>
          {#each g.pages as p (p.id)}<button class="item" class:here={p.id === page?.id} onclick={() => open(p.id)}>{p.title}</button>{/each}
        {/each}
      {/if}
    </nav>
  {/if}
  {#if wide || !showList}
    <article>
      <div class="bar">
        <button onclick={goBack} disabled={!handbook.back.length} title="Back" aria-label="Back">‹</button>
        <button onclick={goForward} disabled={!handbook.forward.length} title="Forward" aria-label="Forward">›</button>
        <span class="crumb">{page?.group} › {page?.title}</span>
        {#if !wide}<button onclick={() => (showList = true)}>Contents</button>{/if}
      </div>
      <!-- page text is escaped by renderMarkdown: only the markup it writes reaches the page -->
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="page" onclick={onClick}>{@html html}</div>
    </article>
  {/if}
</div>

<style>
  .hb { display: flex; height: 100%; overflow: hidden; font-size: 13px; background: var(--panel); color: var(--text); }
  .side { width: 220px; flex: none; overflow: auto; padding: 8px; border-right: 1px solid var(--line); }
  .hb:not(.wide) .side { width: 100%; border: 0; }
  .side input { width: 100%; box-sizing: border-box; margin-bottom: 8px; font: inherit; font-size: 12px; padding: 4px 6px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--field); color: var(--text); }
  .side h4 { margin: 10px 0 4px; font-size: 11px; text-transform: uppercase; color: var(--muted); }
  .item { display: block; width: 100%; text-align: left; font: inherit; background: none; border: 0; padding: 3px 6px; border-radius: 4px; color: inherit; cursor: pointer; }
  .item.here { background: var(--accent-bg); color: var(--accent); }
  .item:hover { background: var(--hover); }
  .none { color: var(--muted); }
  article { flex: 1; min-width: 0; overflow: auto; }
  .bar { position: sticky; top: 0; z-index: 1; display: flex; gap: 6px; align-items: center; padding: 6px 10px; background: var(--panel); border-bottom: 1px solid var(--line); }
  .bar button { font: inherit; font-size: 12px; padding: 4px 10px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); cursor: pointer; }
  .bar button:disabled { opacity: 0.5; cursor: default; }
  .crumb { flex: 1; color: var(--muted); }
  .page { padding: 4px 16px 24px; line-height: 1.5; max-width: 760px; }
  .page :global(a) { color: var(--accent); }
  .page :global(img) { max-width: 100%; border: 1px solid var(--line); border-radius: 4px; }
  .page :global(table) { border-collapse: collapse; }
  .page :global(th), .page :global(td) { border: 1px solid var(--line); padding: 4px 8px; text-align: left; vertical-align: top; }
</style>
