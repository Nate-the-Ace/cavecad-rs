// State every panel shares about the open cave. The cave itself lives in the Rust engine; this
// only says "it changed, look again" so panels refresh together.

export const cave = $state({
  /** Bumps whenever a cave is opened or edited. Panels re-read when it changes. */
  version: 0,
  /** Show least-squares adjusted positions. */
  adjusted: true,
  /** The run the Profile panel is showing. */
  run: '',
});

export function changed() {
  cave.version += 1;
}
