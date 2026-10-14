// Select All (Cmd/Ctrl+A) goes to the view the person last pressed in: the keyboard focus is often on the workspace itself,
// not on a view, so each view registers what its Select All does and says when it was used.

const handlers = new Map<string, () => void>();
let last = '';

/** Registers what Select All does in a view; returns the call that takes it away. */
export function registerSelectAll(view: string, run: () => void): () => void {
  handlers.set(view, run);
  return () => {
    handlers.delete(view);
  };
}

/** Says that a view was just pressed in, so Select All is for it. */
export function markViewUsed(view: string) {
  last = view;
}

/** Runs Select All in the view last used; false when there is none. */
export function selectAllInLastView(): boolean {
  const run = handlers.get(last);
  if (!run) return false;
  run();
  return true;
}
