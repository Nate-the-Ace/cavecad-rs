# New cave: the project folder

Decided 2026-10-26 (Nathan). First slice built 2026-10-26 on branch `new-cave-folder`; see **As built** at the end.

## What changes for the person

Today **New cave…** asks for a name and a unit, and the cave lives only in the browser's shelf. After
this, it also asks **where the cave's folder goes**. The app makes a folder named after the cave
there, with the standard subfolders inside, writes the cave file into it, and opens the cave. From
then on, saving writes to that file and the panels that need a folder (Scans, Import, Export,
backups) use the right subfolder without being asked.

## Decisions

1. **The `.cavecad` file is the cave.** Everything the cave is (trips, shots, drawing, marks, and
   the scan pages it links, which stay bundled by default as today) is in that one file. The folder
   is where the file lives, together with the originals and outputs around it. A `.cavecad` copied
   out of its folder or emailed alone is still the whole cave. Nothing in a subfolder is needed to
   open it. When the file and a subfolder disagree (a scan page edited on disk, say), the file wins
   until the person re-links or re-imports.
2. **Browsers without a folder picker keep the shelf.** Only Chromium browsers (Chrome, Edge) can
   create a folder. On Safari and Firefox, New cave works as it does now (kept in the browser's
   shelf), and **Export → Project** is how they get a folder out. No fallback dialog pretends
   otherwise. The desktop app has no such limit.
3. **The picker opens on the person's Documents folder.** It does not default to Google Drive or
   any other sync folder. The person puts the cave wherever they like, and if they choose a Drive
   folder it syncs. The app neither detects nor prefers any sync folder.

## The folder

```
<Cave Name>/
  <Cave Name>.cavecad   the cave
  scans/                scanned notebook pages, arranged however the person likes
  survey/               raw instrument downloads and imported survey files, kept as they arrived
  images/               photographs
  lidar/                3D captures (.glb .gltf .obj .ply .stl .las .laz .e57 .xyz .pts)
  surface/              aerial imagery, elevation models, GPS tracks
  documents/            landowner agreements, permits, trip reports, the cave description
  trips/                trip plan packets, printable field sheets, leads lists
  exports/              PDF sheets, DXF, Compass/Survex/Therion files, 3D films, packages
  backup/               dated earlier versions of the .cavecad
  .cavecad/             made by the app, safe to delete: previews, scan trims, PDF page splits
```

Every subfolder is created at once, even when empty. An empty `backup/` shows that backups are on,
and an empty `survey/` shows where a download will land.

Names are lowercase, in English, and fixed. They are not translated, because a folder made in one
language has to be found by the app running in another. Reading is case-insensitive, so a person's
`Scans/` counts as `scans/`.

### Where each one comes from

- **scans/**: the Scans panel points here automatically once the cave has a folder. Dropping pages
  onto the panel copies them in. The link paths stored in the cave are relative to `scans/`, as now.
- **survey/**: every Import (Cavway/DistoX/SAP/BRIC pulls, TopoDroid, Compass `.dat`/`.mak`, Survex,
  Walls, Therion, CSV) first copies the original file in, named `<date> <original name>`, and then
  reads it. A bad import, or a reader bug fixed later, can always be redone from the original.
  Instrument pulls are saved as the raw bytes the device sent, beside the parsed shots.
- **images/** and **lidar/**: the person fills them. The 3D viewer and the Package step read them.
- **surface/**: the aerial basemap, the DEM, and any GPS track the Entrance Location tool loads are
  copied here.
- **documents/**: only the person fills it. The app lists it and does nothing else with it.
- **trips/**: Trip Plan writes each packet here as `<date> <trip name>/`.
- **exports/**: the default place for every Export and for Sheets PDFs. The save dialog still lets
  the person choose somewhere else.
- **backup/**: before a save overwrites `<Cave Name>.cavecad`, the previous file is copied here as
  `<Cave Name> <yyyy-mm-dd hhmm>.cavecad`, at most one per ten minutes. The newest 30 are kept, plus
  one a day for older ones. Pruning only ever deletes inside `backup/`.
- **.cavecad/**: anything the app can make again. Nothing in the person's own folders is ever
  app-made, so the old app's filters that hid previews and trims inside `scans/` are not needed.

### Privacy

The first rule still holds: entrance locations never leave the person's machine unless they ask.
`scans/`, `survey/`, `images/`, `lidar/`, `surface/` and `documents/` all count as location data,
since GPS in photos, survey fixes, imagery and landowner names all point at the entrance. A
**sanitized** package leaves all six out unless the person names one. `trips/`, `exports/` and
`backup/` can be included, and the exports themselves already follow the no-coordinates rule.

## The New cave dialog

Fields: **Name**, **Unit** (ft/m, as now), and **Folder** with a **Choose…** button showing where it
will go (`Documents/Truitt Cave`). The folder starts at Documents, and the last folder chosen is
remembered for next time.

- **Desktop (Tauri):** the folder dialog opens with `defaultPath` set to the Documents folder
  (`dirs::document_dir`), or to the last one chosen. Folders are created with `create_dir_all`
  inside the folder the person picked, through the same path-safety rule `files.rs` already applies.
- **Chromium:** `showDirectoryPicker({ startIn: 'documents', mode: 'readwrite', id: 'cave-parent' })`
  picks the place, and the cave folder and subfolders are made with `getDirectoryHandle(name,
  { create: true })`. The handle is stored with the shelf record in IndexedDB, so reopening asks
  only for the browser's permission click.
- **Safari / Firefox:** no Folder field. A line under the name says the cave will be kept in this
  browser and can be exported as a project folder.

Cases:

- **A folder with that name already exists** in the chosen place: if it holds a `.cavecad`, offer
  **Open that cave** or **Choose another place**. If it holds no cave, ask before using it, and then
  only add the subfolders that are missing (as the old `ensureProjectFolders` did). Nothing already
  in it is touched.
- **The name has characters a folder cannot have** (`\ / : * ? " < > |`): the folder name replaces
  them with spaces. The cave's own name stays as typed.
- **Cancel** at the picker returns to the dialog and creates nothing.

## The shelf

A shelf record gains where its folder is: a path (desktop) or a directory handle (Chromium). The
shelf still keeps its own copy, so the list works without reading folders and a cave survives its
folder being unplugged. When the folder is reachable it is the one written to. When it is not, the
shelf says so on that cave's line ("folder not found: Documents/Truitt Cave") and offers **Find
folder…** or **Keep in this browser**. The right-click menu gains **Reveal folder** (desktop, and
Chromium where it can).

## Opening a `.cavecad` that has no folder

A `.cavecad` opened from an email, a USB stick or a download opens as now. The Save line then
offers **Make a project folder…**, which runs the same picker and writes the file into a new
folder. Opening a `.cavecad` that sits inside a folder with the standard subfolders adopts that
folder with no question asked.

## Old caves

A QCAD-era cave folder (`.dwg`/`.dxf`, `scans/`, `PDF/`, `images/`, `backup/`, `lidar/`) imported
into a new cave keeps its folders. `PDF/` is read as `exports/` and is not renamed. Missing
subfolders are added. The old drawing is not moved.

## Not in this slice

Cloud folders through provider APIs (Drive/OneDrive/Dropbox/WebDAV through `cave-store::Store`),
the sanitized/full Package itself, Xref paths (already required to be relative to the cave's
folder), and moving or renaming a cave's folder from inside the app.

## Tests owed

Rust (`apps/desktop/src-tauri/src/files.rs`): folder creation under a temp root, an existing
folder with a cave, an existing folder without one, unsafe names, case-insensitive subfolder
matching, and backup naming and pruning. Web: a `test:newcave` check against a fake directory
handle (subfolders made, file written, handle stored), plus the Safari/Firefox path leaving the
shelf behaviour unchanged.

## As built (2026-10-26)

Rules in `crates/cave-folder` (names, subfolders, backup naming, ten-minute gap, pruning), used by the
desktop app natively (`apps/desktop/src-tauri/src/cavefolder.rs`, commands in `lib.rs`) and by the
browser through `folder_plan` in `cave-wasm`. Browser reading and writing: `lib/folder-handle.ts`.
The open cave's folder and the dispatch between the two: `lib/cavefolder.svelte.ts`. Checks:
`cargo test -p cave-folder`, `cargo test` in `apps/desktop/src-tauri`, `npm run test:newcave`
(an in-memory directory handle with the real rules through WASM).

Verified in Chromium against the browser's private file system standing in for the picked place:
New cave made all nine subfolders plus `.cavecad/` and the file; an edit was written to the file with
one backup; DXF export opened its save dialog in `exports/`; after a reload the shelf showed the
folder and Open read the file from it; with the folder deleted, Open fell back to the shelf copy with
a warning, the shelf line said "Folder not found" and Find folder… re-linked it. The desktop app was
built but its folder dialog was not driven (a native dialog cannot be clicked by the test tools).

Where it differs from the plan above:

- **The file is written on every keep, not only on Save.** The shelf keeps the open cave about a
  second after each edit; a cave with a folder is written to its file at the same moment, so the file
  is never behind the shelf copy (which matters because opening reads the file). The ten-minute rule
  keeps backups from piling up.
- **Desktop exports go straight into `exports/`** with no dialog (the desktop window has no save
  dialog; the message says where the file went). In a browser the save dialog starts in `exports/`.
- **"Make a project folder…" is the ribbon's File → Folder button**, not a line under Save. With a
  folder it reveals it (desktop) or says where it is (browser). It never switches the Scans panel,
  since the pages on show travel inside the file.
- **Chrome will not give a page Documents itself** (nor Desktop or Downloads): it says the folder
  "contains system files" and offers another. Folders inside them are fine. So in a browser the dialog
  starts with nothing chosen and says to pick a folder inside Documents (such as `Documents/Caves`) or
  make one with the picker's New Folder button; a picked folder with the cave's own name is used as the
  cave's folder itself instead of getting a second one inside it. Found live 2026-10-27. The desktop
  app has no such limit.
- **A browser shelf only says "Folder not found" once it has permission to look**; before the first
  click of a session it shows the folder as usual and finds out on Open.

Not built yet: copying imports into `survey/` (the importers live on branch `project-import`),
filling `surface/` and `trips/` (Entrance/basemap and Trip Plan are not ported), the sanitized
package, adopting a QCAD-era cave folder, and adopting the folder of a `.cavecad` opened from a file
(a browser cannot see which folder a picked file is in).
