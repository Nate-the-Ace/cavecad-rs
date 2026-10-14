// The right-click menu on a drawn line. A view opens it with a way to find the lines like the one pressed on.

export const lineMenu = $state({ open: false, x: 0, y: 0, similar: (() => []) as () => string[] });

/** Opens the menu at the screen point; `similar` gives the ids of the lines of the same kind in the view the press was in. */
export function openLineMenu(x: number, y: number, similar: () => string[]) {
  lineMenu.x = x;
  lineMenu.y = y;
  lineMenu.similar = similar;
  lineMenu.open = true;
}

export function closeLineMenu() {
  lineMenu.open = false;
}
