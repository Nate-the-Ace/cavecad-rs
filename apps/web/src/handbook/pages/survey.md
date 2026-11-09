# Survey

The Survey panel takes shots from a survey instrument as you work in the cave and writes them into the notebook, so the map grows while you survey.

This panel only appears when instrument support is switched on, which it is not in ordinary builds.

## When you reach for it

- You are in the cave with an instrument and a tablet.
- You want to try the simulator before a trip.
- You want to see each reading and approve it before it is written.

![The Survey panel](survey.png)

## How to use it

1. Open the cave and choose the trip in the [Notebook](notebook). The trip name shows at the top of the panel.
2. Connect an instrument in the Survey tab, or try the simulator. The badge at the top shows the state, such as **Connected** or **No instrument**.
3. Check the **From** and **To** boxes. They name the station you are at and the one the next shot goes to.
4. Press **Take shot**. With the simulator the button reads **Shoot station**.
5. The big tape, compass and clino numbers show the latest reading.
6. If a reading is held, the panel asks "Write this as" followed by the shot and a question mark. Press **Accept** to write it or **Discard** to drop it.
7. Read the line under the station names. It says what to do next.
8. Under **Passage at** the station, fill in the left, right, up and down distances once the station has a line in the notebook.
9. Press **Next station** to move on without a shot.

The map, profile and cross section redraw as shots are written.

### Keys

- Space takes a shot.
- Enter accepts a held reading.
- Escape discards it.
- Ctrl or Cmd with Z undoes in the notebook.

### Fixing a shot

1. Press **Undo last shot** to take back the newest one.
2. Press **No backsight** if you are waiting for a backsight you will not take.
3. See **This session** for every shot taken so far.

## What can go wrong

| Message or symptom | What it means | Do this |
| --- | --- | --- |
| "Connect an instrument in the Survey tab, or try the simulator." | No instrument is linked. | Connect one, or use the simulator. |
| "This browser has no Web Bluetooth. On an iPad, open CaveCAD in the Bluefy browser to connect an instrument. The simulator works anywhere." | The browser cannot talk to Bluetooth. | Use the Bluefy browser on an iPad, or use the simulator. |
| **Could not connect** | The connection failed. A reason shows in red above. | Try again. |
| **Reconnecting…** | The link dropped. | Wait, or move closer to the instrument. |
| "Waiting for a shot…" | Connected, but no shot yet. | Take a shot on the instrument. |
| "Appears once this station has a line in the notebook (take its shot first)." | Passage sizes need a station with a shot. | Take the shot, then fill them in. |
| "Splays now" | Close shots are waiting to become a leg or splays. | Press **Splays now** to count them as splays. |
| Take shot is greyed out | The panel is not ready for a shot. | Check the From and To names, and the instruction line. |

## Related

- [Notebook](notebook)
- [Map](map)
- [Stations](stations)
