<script lang="ts">
  // Settings > Notebook (phone): what the Notebook tab shows. Backsights off hides the Az bs and Inc bs columns there
  // (the readings stay stored; turning it on brings them back). Not the Survey section's Backsights, which asks the
  // surveyor for a backsight after each leg.
  import { notebookDisplay, setBacksightsShown } from '../lib/notebook-display.svelte';
  import './settings.css';
</script>

<div class="set">
  <label class="chk"><input type="checkbox" checked={notebookDisplay.backsights} onchange={(e) => setBacksightsShown(e.currentTarget.checked)} /> Backsights</label>
</div>
