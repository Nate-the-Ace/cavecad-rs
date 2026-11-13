# Patterns

The Patterns panel is where you look at, copy and make the linetypes, fills and motifs that style your map.

## When you reach for it

- A line style that comes with CaveCAD is close, but not right for your cave.
- You want a hatch or dot fill of your own for an area.
- You want to draw a small repeating shape, such as a tick or a little rock.

![The Patterns panel](patterns.png)

## How to use it

1. Open a cave. Without one you can look at the built-in patterns but not make your own.
2. Pick a tab at the top: **Linetypes**, **Fills** or **Motifs**.
3. Pick a pattern from the list on the left. Each linetype shows a small sample. **Yours** are at the top; under **Comes with CaveCAD** are starting points that match the map's own lines (Presumed wall, Dashed, Underlying passage, Joint, Ledge, Pit edge, Flowline) and a few more.
4. For one that comes with CaveCAD, press **Make my own copy**. You get your own copy to change.
5. To start from nothing, press **+ New** instead.
6. Change the parts in the editor. The preview above them updates as you go.
7. Press **Save**. A new pattern shows **not saved** in the list until you do. If you pick another one first, CaveCAD asks before throwing your changes away.

### Linetypes

1. The **One repeat** strip shows the parts of the pattern in order, each as a block: a **Line**, a **Space**, a **Dot** or a **Motif** (a small shape). Lines and spaces are as wide as they are long.
2. Click a block to pick it. Its settings show in the box underneath: length on paper for a line or space; for a motif, which one, its size, rotation, and a nudge along or sideways.
3. Drag the right edge of a line or space block to make it longer or shorter.
4. Use **+ Line**, **+ Space**, **+ Dot** or **+ Motif** to add a part after the picked one. **◀ Earlier** and **Later ▶** move it; **Remove** takes it out. The arrow keys and Delete work too.
5. Under the preview, switch between a **Curve** and a **Straight** line, and between **Actual size**, **2×** and **4×**.
6. Once saved, select lines on the map and press **Use on selected lines** to give them this linetype. You can also pick it later under **Line style** in the ribbon.

### Fills

1. Under **Comes with CaveCAD** are starting points like the map's own fills (Sand, Pebbles, Clay, Water) and every standard AutoCAD hatch pattern: **ANSI31** to **ANSI38** and the architectural **AR-** patterns (bricks, concrete, herringbone, parquet, roofing, shakes, sand). The AR patterns are drawn at 1/16 of real size, so a brick is about 13 mm on paper.
2. A fill is made of **layers** drawn on top of each other. Each shows as a card with its own small sample; click one to change it in the box underneath.
3. Add a layer with **+ Hatch lines**, **+ Dots**, **+ Scattered motifs** or **+ Pattern lines**. Pattern lines work like a line of an AutoCAD pattern: an angle, a row spacing, how far each row is shifted along, a start point and a dash list (more than 0 a dash, less than 0 a gap, 0 a dot).
4. To mix patterns, pick one under **Add a whole pattern…**: all of its layers go on top of this fill's, and each can then be changed on its own. A fill holds up to 32 layers.
5. Tick **Shade the whole area lightly as well** to colour the area behind the pattern.
6. Slide **Try it denser** to judge the pattern at other spacings. This changes only the preview.
7. Once saved, press **Draw an area with it** and draw round an area, or select areas on the map and press **Use on selected areas**.

#### Importing AutoCAD patterns

1. On the **Fills** tab, pick the file's units: **Millimetres** for acadiso.pat and most metric files, **Inches** for AutoCAD's acad.pat. A file that says it is in inches is read as inches either way.
2. Tick **Real-size patterns** for architectural patterns drawn at real size, so they are shrunk to 1/16.
3. Press **Import .pat…** and pick one or more files. Every pattern in them becomes one of your fills. A name already taken gets "copy" added.

### Motifs

1. Pick **Straight**, **Freehand** or **Circle** and draw in the box. The dashed line across the middle is the line a linetype stands the motif on.
2. For straight lines, click each corner. For freehand, drag; let go and drag again to carry on the same line. Then press **Finish line** (or Enter) to end it, or **Close shape** to join the end back to the start.
3. Tick **Solid** first to have closed shapes and circles filled in.
4. **Undo stroke** takes back the last stroke; **Clear…** starts again.
5. Under the box the motif shows at four sizes and repeated along a line. Press **Save**, then use it in a linetype or fill by adding a Motif part there.

## What can go wrong

| Message or symptom | What it means | Do this |
| --- | --- | --- |
| "Open a cave to make patterns of your own. The ones that come with CaveCAD can be looked at." | No cave is open, so **+ New** is greyed out. | Open or make a cave first. |
| The fields are greyed out and **Save** is missing | You picked a pattern that comes with CaveCAD, which cannot be changed. | Press **Make my own copy**. |
| **Save** is greyed out | Nothing has changed yet, or the pattern has a problem. A red line under the parts says what. | Fix the part it names, or change something. |
| **+ Motif** or **+ Scattered motifs** is greyed out | The cave has no motifs yet. | Make one on the **Motifs** tab first. |
| An imported .pat comes out tiny or huge | The file units or the real-size choice did not match the file. | Delete the imported fills and import again with the other units, or with Real-size patterns ticked. |
| A motif shows "(missing)" | The line or fill uses a motif that no longer exists. | Pick another motif from that row's menu. |

## Related

- [Symbols](symbols)
- [Map](map)
- [Profile](profile)

