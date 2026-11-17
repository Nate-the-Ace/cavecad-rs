<script lang="ts">
  // A thumb stick: drag from the middle; reports x and y in −1…1 (y up positive) while held, 0,0 on release.
  let { onmove, disabled = false, size = 168 }: { onmove: (x: number, y: number) => void; disabled?: boolean; size?: number } = $props();
  let base: HTMLDivElement;
  let dx = $state(0), dy = $state(0), active = $state(false);
  const r = $derived(size / 2 - 26);
  function at(e: PointerEvent) {
    const b = base.getBoundingClientRect();
    let x = e.clientX - (b.left + b.width / 2), y = e.clientY - (b.top + b.height / 2);
    const d = Math.hypot(x, y);
    if (d > r) { x *= r / d; y *= r / d; }
    dx = x; dy = y;
    // a small dead zone so a resting thumb does not creep
    const dz = (v: number) => (Math.abs(v) < 0.08 ? 0 : v);
    onmove(dz(x / r), dz(-y / r));
  }
  function down(e: PointerEvent) {
    if (disabled) return;
    base.setPointerCapture(e.pointerId);
    active = true;
    at(e);
  }
  function up() {
    if (!active) return;
    active = false;
    dx = dy = 0;
    onmove(0, 0);
  }
</script>

<div class="stick" class:active class:disabled bind:this={base} style="width:{size}px;height:{size}px"
  onpointerdown={down} onpointermove={(e) => active && at(e)} onpointerup={up} onpointercancel={up} onlostpointercapture={up}
  oncontextmenu={(e) => e.preventDefault()} role="slider" aria-label="Aim: left and right pan, up and down roll" aria-valuenow={0} tabindex="-1">
  <span class="lab t">roll</span><span class="lab l">pan</span>
  <div class="thumb" style="transform: translate({dx}px, {dy}px)"></div>
</div>

<style>
  .stick { position: relative; border-radius: 50%; margin: 6px auto; touch-action: none; user-select: none; -webkit-user-select: none;
    -webkit-touch-callout: none; background: radial-gradient(circle, var(--panel-2, #8881) 0 60%, var(--panel-3, #8882) 61%);
    border: 1px solid var(--border, #8884); }
  .stick.disabled { opacity: 0.45; }
  .thumb { position: absolute; left: 50%; top: 50%; width: 52px; height: 52px; margin: -26px 0 0 -26px; border-radius: 50%;
    background: var(--accent, #2b7de9); box-shadow: 0 2px 6px #0004; transition: transform 0.12s ease-out; }
  .stick.active .thumb { transition: none; }
  .lab { position: absolute; font-size: 11px; opacity: 0.6; pointer-events: none; }
  .lab.t { top: 6px; left: 50%; transform: translateX(-50%); }
  .lab.l { left: 8px; top: 50%; transform: translateY(-50%); }
</style>
