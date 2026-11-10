<script lang="ts">
  // A picked viewframe's settings in the ribbon: its scale, its lock, working inside it, deleting it.
  import { Lock, LockOpen, Maximize2, Trash2 } from '@lucide/svelte';
  import { cave } from '../lib/cave.svelte';
  import { engine } from '../lib/engine';
  import { editFrame, pickFrame, pickedFrame, scaleRatio, vf } from '../lib/viewframe.svelte';

  let { part }: { part: string; kind?: string } = $props();
  const picked = $derived.by(() => {
    void cave.version;
    void vf.frame;
    return pickedFrame();
  });
  const scales = $derived.by(() => {
    try {
      return engine.sheetOptions().scales;
    } catch {
      return [];
    }
  });
  const fpi = $derived(picked?.f.feet_per_inch ?? 0);
  const locked = $derived(!!picked?.f.locked);
  const listed = $derived(scales.some((s) => Math.abs(s.feet_per_inch - fpi) < 1e-6));

  function setScale(v: string) {
    const n = Number(v);
    if (n > 0) editFrame((f) => void (f.feet_per_inch = n));
  }
  function typed(v: string) {
    // "1:250" or "250": paper to ground, so feet per inch is N / 12
    const m = v.trim().match(/^(?:1\s*:\s*)?(\d+(?:\.\d+)?)$/);
    if (m && Number(m[1]) > 0) editFrame((f) => void (f.feet_per_inch = Number(m[1]) / 12));
  }
</script>

{#if !picked}
  <div class="row muted">No viewframe picked.</div>
{:else if part === 'scale'}
  <div class="col">
    <label>Scale
      <select value={listed ? String(fpi) : ''} disabled={locked} onchange={(e) => setScale(e.currentTarget.value)} title={locked ? 'Locked: unlock the frame to change its scale' : 'The frame\'s plot scale'}>
        {#if !listed}<option value="">{scaleRatio(fpi)} (custom)</option>{/if}
        {#each scales as s}<option value={String(s.feet_per_inch)}>{s.label}</option>{/each}
      </select>
    </label>
    <label>Custom
      <input type="text" value={scaleRatio(fpi)} disabled={locked} onchange={(e) => typed(e.currentTarget.value)} title="Type a scale such as 1:250" size="8" />
    </label>
  </div>
{:else if part === 'lock'}
  <div class="row">
    <button class="tool" class:pressed={locked} aria-pressed={locked} title={locked ? 'Unlock: let the scale and view change' : 'Lock the scale and view: the frame can still be moved and resized'} onclick={() => editFrame((f) => void (f.locked = !f.locked))}>
      {#if locked}<Lock size={22} strokeWidth={1.6} />{:else}<LockOpen size={22} strokeWidth={1.6} />{/if}
      <span>{locked ? 'Locked' : 'Unlocked'}</span>
    </button>
    <button class="tool" class:pressed={vf.inside} aria-pressed={vf.inside} title="Work inside the frame: drag pans its map, the wheel zooms it (or double-click inside the frame). Esc to come back out" onclick={() => (vf.inside = !vf.inside)}>
      <Maximize2 size={22} strokeWidth={1.6} /><span>{vf.inside ? 'Inside' : 'Focus inside'}</span>
    </button>
    <button class="tool" disabled={picked.f.id === 'main' && picked.def.frames.length === 1} title="Delete this viewframe" onclick={() => { const id = vf.frame; editFrame((f, def) => void (def.frames = def.frames.filter((q) => q.id !== id))); pickFrame('', ''); }}>
      <Trash2 size={22} strokeWidth={1.6} /><span>Delete</span>
    </button>
  </div>
{/if}

<style>
  .row { display: flex; align-items: center; gap: 4px; height: 100%; font-size: 12px; }
  .col { display: flex; flex-direction: column; justify-content: center; gap: 4px; height: 100%; font-size: 12px; }
  .col label { display: flex; align-items: center; gap: 6px; justify-content: space-between; }
  .muted { color: var(--muted); }
  .tool { display: flex; flex-direction: column; align-items: center; gap: 4px; min-width: 58px; box-sizing: border-box; height: calc(var(--ribbon-body) - 1px); justify-content: center; overflow: hidden; padding: 4px 8px; background: none; border: 0; border-radius: var(--radius); cursor: pointer; font-size: 12px; color: var(--text); }
  .tool:hover { background: var(--panel-2); }
  .tool.pressed { background: var(--accent-bg); outline: 1px solid var(--accent); }
  .tool:disabled { opacity: 0.45; cursor: default; }
</style>
