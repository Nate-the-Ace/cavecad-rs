<script lang="ts">
  import { relay, saveRelay, testRelay } from '../lib/p2p-relay.svelte';
</script>

<details class="relay">
  <summary>Relay for strict networks (optional)</summary>
  <p class="fine">
    Some networks block direct connections. A relay server (for example a free Metered account) passes the already-encrypted data along as a fallback; it cannot read it. Direct is always tried first. These details stay in this browser.
  </p>
  <label>Credentials link <input type="url" placeholder="https://yourapp.metered.live/api/v1/turn/credentials?apiKey=…" bind:value={relay.url} oninput={saveRelay} autocomplete="off" spellcheck="false" /></label>
  <p class="fine">Or by hand:</p>
  <label>Relay addresses <textarea rows="2" placeholder="turn:…:80, turns:…:443?transport=tcp" bind:value={relay.urls} oninput={saveRelay} spellcheck="false"></textarea></label>
  <div class="two">
    <label>Username <input bind:value={relay.username} oninput={saveRelay} autocomplete="off" spellcheck="false" /></label>
    <label>Password <input type="password" bind:value={relay.credential} oninput={saveRelay} autocomplete="off" /></label>
  </div>
  <div class="row"><button onclick={testRelay} disabled={relay.testing}>{relay.testing ? 'Testing…' : 'Test relay'}</button><span role="status">{relay.result}</span></div>
</details>

<style>
  .relay { font-size: 12px; }
  summary { cursor: pointer; }
  label { display: block; margin-top: 6px; }
  input, textarea { width: 100%; box-sizing: border-box; font: inherit; }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .row { display: flex; align-items: center; gap: 10px; margin-top: 8px; }
  .fine { margin: 6px 0 0; opacity: 0.75; }
</style>
