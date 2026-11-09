// The `?` on a panel and the dialogs' "Learn more" links open a page here and bring the Handbook panel to the front.
import { goTo } from './handbook.svelte';
import { showPanel } from './workspace.svelte';

/** Opens the Handbook panel at a page. */
export function openHandbook(id: string) {
  goTo(id);
  showPanel('handbook');
}
