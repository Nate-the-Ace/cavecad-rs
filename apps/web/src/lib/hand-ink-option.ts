// Whether Ink style is on (the View tab's switch, kept with the other view options). The reader is
// installed by `cave.svelte.ts`: importing that module from here would close an import ring through
// the drawing code (`section-glyph` -> here -> `cave.svelte` -> `section-glyph`).

// `var` with no initial value: it may be installed while the modules are still loading.
var read: (() => boolean) | undefined;

/** Installs where the switch is read from (call once, from `cave.svelte.ts`). */
export function installHandInk(f: () => boolean) {
  read = f;
}

/** True when lines are drawn hand-inked. */
export function handInkOn(): boolean {
  return read?.() ?? false;
}
