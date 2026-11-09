# Bringing survey data in

A browser only gives CaveCAD the files you pick, never the folder around them: anything spread over several files comes in as a whole folder or a single ZIP.

Every import replaces the cave that is open. Start from an empty cave, or save first.

## Pick the right path

| What you have | Path |
| --- | --- |
| A TopoDroid survey (shots and sketches) | Export the survey as a ZIP on the phone, then **Open** it (procedure A) |
| A Therion project: a top file (`main.th` or `thconfig`) that pulls in other files | **Import › Project folder…**, pick the folder that holds the top file (procedure B) |
| A Compass project (`.mak` and `.dat` files) or a Walls project (`.wpj` and `.srv` files) | **Import › Project folder…** (procedure B) |
| One self-contained survey file (`.dat`, `.srv`, `.svx`, `.th`, `.csv`) | **Import › Survey files…** or **Open** (procedure C) |
| A TopoDroid Therion export: a `.th` and its sketch `.th2` | **Import › Survey files…**, pick both together (procedure C) |
| Only a `.th2` sketch | Not enough on its own: export the survey too (A, or the `.th` in C) |

## A: a TopoDroid survey (ZIP)

The ZIP is the one TopoDroid export that carries the shots and every sketch together. The sketch export menu (Therion, cSurvey, DXF, SVG…) gives one sketch with no shots.

1. In TopoDroid, open the survey's shot list (not a sketch).
2. Open the menu, choose **Export**, then **ZIP**. Save or share the file where your computer can reach it.
3. In CaveCAD, choose **Open** on the Home tab, or **Import file…** in the Caves window. Pick the `.zip`.

What comes in: the shots (repeated readings of a leg averaged, deleted and commented-out shots left out), the declination (worked out from the GPS fix and date when the survey has none), the location, plan sketches on the [Map](map), profile sketches on the [Profile](profile), cross-section sketches on their station's [Cross Section](section). Photos and audio notes stay behind.

## B: a project folder (Therion, Compass, Walls, Survex)

Use this whenever the survey lives in more than one file. Picking only the top file fails: CaveCAD finds every file it pulls in missing.

1. In the Caves window choose **Start empty**, or **New cave…** to keep the cave in a folder of its own.
2. Choose **Import › Project folder…** on the Home tab.
3. Pick the folder that holds the project's top file, not a folder inside it. Chrome refuses Documents, Desktop and Downloads themselves: keep projects in a folder inside one of them.
4. CaveCAD reads every survey and sketch file in that folder and its subfolders, then follows the project file. With no project file, every survey file found is merged into one cave.

## C: single files, and a TopoDroid Therion pair

1. Choose **Import › Survey files…** on the Home tab. **Open** also takes a single survey file.
2. To bring in several, select them together in the picker; they must sit in the same folder.
3. A TopoDroid Therion export is two files: the survey's `.th` and the sketch's `.th2`. Pick both together.

## Check the import

An import that reads a format detail wrong does not fail; it draws a plausible but wrong map. Before working on the cave:

- Read every line the import wrote in the [Console](console).
- Check the surveyed length, station count and trips in the Caves window against the source program.
- On the [Map](map), sketches sit on the survey lines, walls either side of the passage.
- The loop lines at the foot of the [Notebook](notebook) match the loops the source program reports.
- The declination in the [Notebook](notebook) is the one you expect.
- Save the cave: it is not a CaveCAD file until you do.

## What can go wrong

| Message | What it means | Do this |
| --- | --- | --- |
| "… is a Therion project's top file: its survey is in N other files it pulls in…" | The top file was picked without the files it pulls in | Procedure B |
| "A .th2 file is only the sketch: it has no survey shots to draw onto…" | Only a sketch was picked | Procedure A, or C with the `.th` too |
| "The sketch page … was not among the files picked" | The survey names a sketch that was not picked | Pick it too, or use procedure B |
| "scrap … was not placed: none of the stations it marks are in the survey" | A sketch's station names match nothing in the survey | Check the survey holding those stations came in |
| "… is not a CaveCAD file" (on a `.zip`) | A zip that is neither a CaveCAD cave nor a TopoDroid export | A TopoDroid ZIP comes from the survey's Export › ZIP |
| "Types this app does not know yet…" | Symbols or line types with no CaveCAD equivalent | Lines are kept as plain sketch lines, points are left out |

## Related

- [Map](map)
- [Profile](profile)
- [Cross Section](section)
- [Notebook](notebook)
- [Console](console)
