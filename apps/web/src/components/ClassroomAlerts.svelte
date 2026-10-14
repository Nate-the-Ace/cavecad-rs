<script lang="ts">
  // What a teacher cannot miss: a card on the screen when a student raises a hand (it stays until the hand
  // comes down or the teacher calls on them) or writes a line with an emoji in it (it fades after a few seconds).
  import { callOn, classroom, dismissAlert, setChime } from '../lib/classroom.svelte';
</script>

{#if classroom.role === 'teacher' && !classroom.ended}
  <div class="alerts" data-classroom-ui aria-live="assertive">
    {#each classroom.alerts as a (a.id)}
      {#if a.kind === 'hand'}
        <div class="card hand" role="alert">
          <span class="big wave">✋</span>
          <span class="who"><b>{a.who}</b> has a hand up</span>
          <button class="go" onclick={() => (callOn(a.whoId), dismissAlert(a.id))}>Call on</button>
          <button onclick={() => dismissAlert(a.id)} aria-label="Dismiss">×</button>
        </div>
      {:else}
        <div class="card emoji" role="status">
          <span class="who"><b>{a.who}</b></span>
          <span class="say">{a.text}</span>
          <button onclick={() => dismissAlert(a.id)} aria-label="Dismiss">×</button>
        </div>
      {/if}
    {/each}
    {#if classroom.alerts.some((a) => a.kind === 'hand')}
      <label class="chime"><input type="checkbox" checked={classroom.chime} onchange={(e) => setChime(e.currentTarget.checked)} /> chime</label>
    {/if}
  </div>
{/if}

<style>
  .alerts { position: fixed; top: 54px; left: 50%; transform: translateX(-50%); z-index: 70; display: flex; flex-direction: column; align-items: center; gap: 8px; pointer-events: none; }
  .card { pointer-events: auto; display: flex; align-items: center; gap: 12px; padding: 10px 14px; border-radius: 12px; background: var(--panel); color: var(--text); border: 2px solid var(--accent); box-shadow: 0 6px 24px rgba(0, 0, 0, 0.45); animation: drop 0.25s ease-out; font-size: 15px; }
  .card.hand { border-color: var(--warn, #f0a020); animation: drop 0.25s ease-out, glow 1.4s ease-in-out infinite; }
  .big { font-size: 30px; line-height: 1; }
  .wave { display: inline-block; transform-origin: 70% 80%; animation: wave 1s ease-in-out infinite; }
  .say { font-size: 22px; max-width: 360px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  button { font: inherit; font-size: 13px; padding: 4px 10px; border: 1px solid var(--line); border-radius: 6px; background: var(--panel-2); color: var(--text); cursor: pointer; }
  button.go { background: var(--accent); border-color: var(--accent); color: #fff; }
  .chime { pointer-events: auto; font-size: 11px; color: var(--muted); display: flex; align-items: center; gap: 4px; }
  @keyframes drop { from { opacity: 0; transform: translateY(-14px) scale(0.96); } to { opacity: 1; transform: none; } }
  @keyframes glow { 0%, 100% { box-shadow: 0 6px 24px rgba(0, 0, 0, 0.45), 0 0 0 0 rgba(240, 160, 32, 0.6); } 50% { box-shadow: 0 6px 24px rgba(0, 0, 0, 0.45), 0 0 0 10px rgba(240, 160, 32, 0); } }
  @keyframes wave { 0%, 100% { transform: rotate(0); } 25% { transform: rotate(18deg); } 75% { transform: rotate(-14deg); } }
  @media (prefers-reduced-motion: reduce) { .card, .wave { animation: none !important; } }
</style>
