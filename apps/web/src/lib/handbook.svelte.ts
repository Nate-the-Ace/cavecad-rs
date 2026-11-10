// Where the Handbook is: the page shown and the way back and forward. Kept free of the workspace so the panel can
// import it; `openHandbook` (which brings the panel to the front) lives in handbook-open.ts.

export const handbook = $state<{ page: string; back: string[]; forward: string[] }>({ page: 'bringing-survey-data-in', back: [], forward: [] });

/** Shows a page, remembering the one before it. */
export function goTo(id: string) {
  if (id === handbook.page) return;
  handbook.back.push(handbook.page);
  handbook.forward = [];
  handbook.page = id;
}

export function goBack() {
  const prev = handbook.back.pop();
  if (prev === undefined) return;
  handbook.forward.push(handbook.page);
  handbook.page = prev;
}

export function goForward() {
  const next = handbook.forward.pop();
  if (next === undefined) return;
  handbook.back.push(handbook.page);
  handbook.page = next;
}
