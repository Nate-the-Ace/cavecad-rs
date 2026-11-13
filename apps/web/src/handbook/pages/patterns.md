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

1. Each layer is **Hatch lines**, **Dots** or **Motif tiles**. Add one with **+ Hatch**, **+ Dots** or **+ Motif tiles**.
2. Set the angle and spacing for hatch lines, or the spacing and dot size for dots.
3. Tick **tint the whole area** to colour the area behind the pattern.
4. Slide **denser** to judge the pattern at other spacings. This changes only the preview.

### Motifs

1. Pick **Straight**, **Freehand** or **Circle** and draw in the box.
2. Press **Close shape** to join the end of a line back to its start.
3. Use **Undo stroke** or **Clear** to fix mistakes, then press **Save motif**.

## What can go wrong

| Message or symptom | What it means | Do this |
| --- | --- | --- |
| "Open a cave to make patterns of your own. The ones that come with CaveCAD can be looked at." | No cave is open, so **+ New** is greyed out. | Open or make a cave first. |
| The fields are greyed out and **Save** is missing | You picked a pattern that comes with CaveCAD, which cannot be changed. | Press **Make my own copy**. |
| **Save** is greyed out | Nothing has changed yet, or the pattern has a problem. A red line under the parts says what. | Fix the part it names, or change something. |
| **+ Motif** or **+ Motif tiles** is greyed out | The cave has no motifs yet. | Make one on the **Motifs** tab first. |
| A motif shows "(missing)" | The line or fill uses a motif that no longer exists. | Pick another motif from that row's menu. |

## Related

- [Symbols](symbols)
- [Map](map)
- [Profile](profile)
