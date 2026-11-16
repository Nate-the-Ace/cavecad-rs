<script lang="ts">
  // A Handbook page shown on top of everything, for a quick look without leaving what you are doing (the Cavway X1
  // quick reference from the Capture panel). Full screen on a phone, a window on anything bigger. The page text is
  // escaped by renderMarkdown, so only the markup it writes reaches the screen.
  import { X } from '@lucide/svelte';
  import { catalog, shotUrl } from '../handbook/bundle';
  import { renderMarkdown } from '../handbook/markdown';

  let { page, onclose }: { page: string; onclose: () => void } = $props();
  const p = $derived(catalog.byId(page));
  const html = $derived(p ? renderMarkdown(p.text, (kind, t) => (kind === 'image' ? shotUrl(t) : null)) : '');
</script>

<div class="veil" role="presentation" onclick={onclose}>
  <div class="box" role="dialog" aria-modal="true" aria-label={p?.title ?? 'Quick reference'} tabindex="-1" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.key === 'Escape' && onclose()}>
    <header>
      <b>{p?.title ?? 'Quick reference'}</b>
      <button onclick={onclose} aria-label="Close"><X size={20} /> Close</button>
    </header>
    <!-- eslint-disable-next-line svelte/no-at-html-tags: escaped by renderMarkdown -->
    <div class="page">{@html html}</div>
  </div>
</div>

<style>
  .veil { position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55); display: grid; place-items: center; z-index: 70; }
  .box { width: min(760px, 94vw); height: min(88vh, 900px); background: var(--panel); color: var(--text); border: 1px solid var(--line); border-radius: 10px; display: flex; flex-direction: column; outline: none; overflow: hidden; }
  header { display: flex; align-items: center; justify-content: space-between; gap: 8px; padding: 8px 12px; border-bottom: 1px solid var(--line); }
  header button { display: inline-flex; align-items: center; gap: 4px; font: inherit; min-height: 40px; padding: 4px 12px; border: 1px solid var(--line); border-radius: var(--radius); background: var(--panel-2); color: var(--text); }
  .page { flex: 1; overflow: auto; padding: 4px 16px 16px; font-size: 14px; line-height: 1.45; }
  .page :global(h1) { font-size: 18px; margin: 10px 0 6px; }
  .page :global(h2) { font-size: 15px; margin: 16px 0 6px; border-bottom: 1px solid var(--line); padding-bottom: 2px; }
  .page :global(h3) { font-size: 14px; margin: 12px 0 4px; }
  .page :global(p), .page :global(ul), .page :global(ol) { margin: 0 0 8px; }
  .page :global(ul), .page :global(ol) { padding-left: 1.3em; }
  .page :global(table) { border-collapse: collapse; margin: 0 0 8px; width: 100%; }
  .page :global(td), .page :global(th) { border: 1px solid var(--line); padding: 4px 8px; text-align: left; vertical-align: top; }
  .page :global(th) { background: var(--panel-2); }
  .page :global(code) { font-family: var(--mono, ui-monospace, monospace); }
  @media (max-width: 600px), (max-height: 500px) {
    .box { width: 100vw; height: 100dvh; border-radius: 0; border: 0; padding-top: env(safe-area-inset-top); padding-bottom: env(safe-area-inset-bottom); box-sizing: border-box; }
  }
</style>
