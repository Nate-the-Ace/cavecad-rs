# Notebook

The Notebook lays one survey trip out like a field-book page, so you can type shots in and fix readings.

## When you reach for it

- You are copying a paper survey page into CaveCAD.
- You want to correct a tape, compass or clino reading.
- You want to check loop closures for a trip.
- You want to set a trip's name, date, team, instruments or declination.

![The Notebook panel](notebook.png)

## How to use it

1. Open the **Trip** list at the top and pick a trip. Click **New trip** to start a new page.
2. Fill in **Name**, **Date** (as YYYY-MM-DD), **Team** and **Instruments**. Press Enter to keep each one.
3. Type the station where the shot starts in **From**. It is filled in for you after the first shot.
4. Type the station where it ends in **To**, then press Enter or click **Add shot**.
5. The new shot appears with the cursor in its tape cell. Type the tape, then the compass, then the clino. Tab moves to the next cell.
6. Press Enter after the last reading. The next shot starts, with **From** and **To** already filled in.
7. Under each station, type the passage dimensions in the L, R, U and D columns. These are left, right, up and down.
8. Use **Undo in the notebook** if you make a mistake.

### Backsights

Tick **Backsights** to show the backsight columns. A trip that already has backsights shows them anyway.

### Declination

1. Type the trip's declination in **Decl.**, in degrees, east positive.
2. Every bearing in the trip turns by it. The compass cells keep what was read.

### Splays and branches

- Click **Add splay** for a reading to a wall with no station at the far end.
- Type an existing station in **To** and the shot joins it, which makes a loop.
- Type a different station in **From** to start a branch.
- A hatched gap between rows is a chain breaking.

### Delete or find a shot

- Click the bin on a shot's row, titled **Delete this shot**, to remove it.
- Right-click a station or shot for more choices, such as **Next shot from** that station.
- Click a row to pick it on the [Map](map).

### Check loops

Loop closures appear as buttons at the foot. Click one to jump to its row. It says how far off the loop is in plan, as a length and a percentage.

## What can go wrong

| Message or symptom | What it means | Do this |
| --- | --- | --- |
| "Not a length." | The tape cell is not a number | Type a length, such as 12.4 |
| "A compass reading is 0 to 360." | The compass cell is out of range | Type a bearing from 0 to 360 |
| "A clino reading is -90 to 90." | The clino cell is out of range | Type a slope from -90 to 90 |
| "A dimension is a length, or P for open passage." | An L, R, U or D cell is not a length | Type a length, or P where the passage is open |
| "Declination is degrees, east positive." | The declination is not a number | Type degrees, such as 4.5 |
| "no date" beside the totals | The trip has no date | Type one in **Date** |
| "N to check" beside the totals | Some shots have a problem | Look for rows outlined in the warning colour |
| "Open a cave to see its notebook." | No cave is open | Open a cave |
| "This trip has no shots yet." | The trip is empty | Type **From** and **To**, then click **Add shot** |

## Related

- [Stations](stations)
- [Map](map)
- [Scans](scans)
- [Bringing survey data in](bringing-survey-data-in)
