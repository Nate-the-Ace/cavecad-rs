// State every panel shares about the open cave. The cave itself lives in the Rust engine; this
// only says "it changed, look again" so panels refresh together.

export const cave = $state({
  /** Bumps whenever a cave is opened or edited. Panels re-read when it changes. */
  version: 0,
  /** Show least-squares adjusted positions. */
  adjusted: true,
  /** Bumps when a whole cave is opened from a file, so the canvas re-reads and re-frames. */
  opened: 0,
  /** Name of the file the cave came from or will be saved to. */
  fileName: 'cave.dxf',
  /** The run the Profile panel is showing. */
  run: '',
});

/** What the canvas hides, by type and by trip. A view setting only: nothing is stored on the drawing. */
export const filter = $state({ hiddenTypes: [] as string[], hiddenTrips: [] as string[] });

/** The filter in the shape the engine reads. */
export function wireFilter(): string {
  return JSON.stringify({ hidden_types: filter.hiddenTypes, hidden_trips: filter.hiddenTrips });
}

/** Shows or hides one type (`on` = shown) or one trip. */
export function setShown(kind: 'types' | 'trips', key: string, on: boolean) {
  const list = kind === 'types' ? filter.hiddenTypes : filter.hiddenTrips;
  const next = list.filter((k) => k !== key);
  if (!on) next.push(key);
  if (kind === 'types') filter.hiddenTypes = next;
  else filter.hiddenTrips = next;
}

export function changed() {
  cave.version += 1;
}
