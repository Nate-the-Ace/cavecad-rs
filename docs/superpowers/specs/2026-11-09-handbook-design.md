# CaveCAD Handbook (in-app) — design

2026-11-09. Approved by Nathan in conversation. First slice only; later slices are listed at the end.

## Why

The import workflows proved not intuitive (picking `main.th` alone, a lone `.th2`, where the TopoDroid ZIP
goes). Nathan wants SOPs, then a handbook. The QCAD-era CaveCAD had an in-app Handbook (65 pages, a `?` on
each panel, coverage tests); the Rust app has none.

## Decisions (Nathan's)

- **Inside the app.** Not a separate site and not shared docs: it works offline in a cave and ships with
  every build, so it cannot drift from the app.
- **A reference for every tool first.** One page per tool, reached from a `?` on it. Lessons (a Start Here
  course) come later.
- **A screenshot on every tool page**, captured from the real app, flagged when older than the tool it shows.

## What the reader sees

- A **Handbook** panel, registered in `PANELS` (`apps/web/src/lib/workspace.svelte.ts`) like Map or Notebook,
  so it docks, tabs, floats, and appears in the View tab's toggles.
- One page at a time: back arrow, a breadcrumb, then the page. Below the page: Contents (pages by group)
  and a search box (title and body words). Chosen over a tree beside the page because a docked column is
  narrow and the screenshot needs the width (same reasoning as the old app).
- A **`?`** in every panel's tab (beside its title) opens that panel's page in the Handbook panel, opening the
  Handbook panel if it is closed. The four big dialogs (New cave, Import, Surface Data, Pick entrance) get a
  "Learn more" link in their footer.
- Links between pages; every page ends with **Related** links.

## Pages

- Markdown files in `apps/web/src/handbook/pages/<id>.md`, edited as plain text by anyone. Built into the
  app at build time (Vite `?raw` import), rendered by a small Markdown subset: headings, paragraphs, bullet
  and numbered lists, `code`, bold, links `[text](page-id)`, images, tables. No HTML in pages.
- `apps/web/src/handbook/index.json`: page order (= reading order), group, title, and for a tool page the
  panel id or command id it documents. Groups in the first slice: **Panels**, **Tasks**.
- A **tool page** has these sections, in this order, all required:
  1. one sentence: what it is for
  2. **When you reach for it**
  3. the screenshot
  4. **How to use it**: numbered steps
  5. **What can go wrong**: the exact messages the app shows, what each means, what to do
  6. **Related**
- A **task page** (how do I...) is steps first, linking the tool pages it uses. The import SOP
  ("SOP: Bringing survey data into CaveCAD", Claude Doc 7oNrWf58nc6mexW3RVUVuG) becomes the first task page,
  `bringing-survey-data-in`.

## Screenshots

- `apps/web/src/handbook/shots/<page-id>.png`, one per tool page.
- Captured by `tools/handbook-shots.mjs`: starts nothing itself; given a running dev server, it drives a
  headless Chrome (`--headless=new --screenshot`, no new npm dependency) at
  `/?handbook-shot=<panel-id>`, a capture mode in the app that opens the Pitfall Cave sample, lays out only
  that panel at a fixed size, hides the sign-in gate and the Caves window, and selects a representative item
  (a run in Profile, a station in Cross Section, a trip in Notebook).
- `apps/web/src/handbook/shots.json` records, per shot, the hash of the panel's source file at capture time.

## Keeping it honest (tests, `tests/handbook.check.ts`, part of `npm test`)

The build fails when:
- a panel in `PANELS` has no page, or a page names a panel or command that no longer exists;
- a link points at no page, or an image at no file;
- a tool page lacks one of its six sections;
- a shot's recorded hash differs from its panel's current source (the shot is stale: re-run the script).
  Staleness fails only in the release workflow, not on every local run, so a code change is not blocked
  until its picture is redone (the old app's rule).

## First slice

The Handbook panel, the `?` on panel tabs, the "Learn more" links, the Markdown renderer, the capture mode
and script, the tests, and pages for: Map, Profile, Cross Section, Symbols, Stations, Notebook, Survey,
Scans, Sheet, Patterns, Console, Rooms (12 panel pages) and `bringing-survey-data-in` (1 task page).
Survey is shown only where instrument support is switched on (`showInstruments`); its page exists either way.

## Later slices (not designed here)

Ribbon tool and dialog pages; concept pages ("what the words mean": run, trip, declination, loop); a Start
Here course built from the pages; the same pages published on cavecad.com for people who have not installed
the app.
