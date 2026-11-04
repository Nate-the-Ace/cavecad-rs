// The right-click menu: one menu for the whole app. Whatever was pressed on (a drawn line, a station,
// empty space in a view, a row of the notebook or the station table) opens it with its own list of
// what can be done there; `context-items.ts` builds the lists the views share.

/** One line of the menu: an action, a heading naming what was pressed on, or a separator. */
export type MenuEntry = { label: string; run: () => void; disabled?: boolean; danger?: boolean; keys?: string } | { heading: string } | 'separator';

export const contextMenu = $state({ open: false, x: 0, y: 0, items: [] as MenuEntry[] });

/** Opens the menu at the screen point with `items` (separators at either end or doubled are dropped). Nothing opens for an empty list. */
export function openContextMenu(x: number, y: number, items: MenuEntry[]) {
  const tidy: MenuEntry[] = [];
  for (const it of items) if (it !== 'separator' || (tidy.length && tidy[tidy.length - 1] !== 'separator')) tidy.push(it);
  while (tidy[tidy.length - 1] === 'separator') tidy.pop();
  if (!tidy.some((it) => it !== 'separator' && 'run' in it)) return;
  contextMenu.x = x;
  contextMenu.y = y;
  contextMenu.items = tidy;
  contextMenu.open = true;
}

export function closeContextMenu() {
  contextMenu.open = false;
}

/** How a shortcut reads on this computer: ⌘ on a Mac, Ctrl elsewhere. */
export const shortcut = (key: string, shift = false): string => {
  const mac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform);
  return mac ? `${shift ? '⇧' : ''}⌘${key}` : `Ctrl+${shift ? 'Shift+' : ''}${key}`;
};
