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

### Shot settings in the Survey tab

- With an instrument that does not flag legs itself, **3 close shots are a leg, 1 is a splay**: three or more consecutive close shots make a leg, any other shot is a splay. An X1 also flags legs itself and can mark a leg Backsight with its **M** button; the other instruments cannot, so a leg that comes back along the one before is taken as its backsight.
- **LRUD**: the next shots are wall dimensions of the station (aim left, right, up or down), not splays.
- **Backsights**: take a backsight after each leg, from the new station looking back.
- **Write at once** off: each shot waits for **Accept** in this panel before it is written.

### Manual or Digital

The two buttons under **Device control** set how much detail the instrument captures.

- **Manual**: you sketch the detail. LRUD is four aimed shots, one to each wall: left, right, up, down. CaveCAD says which wall is next.
- **Digital**: the Cavway captures the detail. Sweep every station to 100% covered: a ring across the passage, rooms all the way round.

### Sweeps on the X1

- **LRUD ring** or **Room sweep** starts a sweep from the panel.
- You can instead hold DIST+M on the X1 for its own scan mode. The **X1 scan (DIST+M) checks as** box says whether CaveCAD checks that scan as an LRUD ring or a room sweep.
- While the panel waits for the X1's scan: for a room, keep the laser about level and turn right round; for a ring, sweep slowly across the passage. End the scan on the X1 (CLR) when the check here says complete: CaveCAD finishes the sweep by itself.

### X1 marks

Mark a shot on the X1 with **M** while the laser is on. The splay is coloured by its mark; under **X1 marks** you choose what a Feature, Ridge or Generic mark also draws (a line through the hits, or a symbol).

### Capture, for an instrument that is not supported yet

Under **Instrument log** at the bottom, capture connects to any Bluetooth device and logs every frame it sends. The browser can only reach services it is told about in advance: list the service UUIDs (from a Bluetooth scanner app such as nRF Connect), separated by spaces or commas, then press **Connect in capture mode**.

## What can go wrong

| Message or symptom | What it means | Do this |
| --- | --- | --- |
| "No reading yet." | No instrument is linked, or nothing has been shot. | Connect one in the Survey tab, or use the simulator. |
| "No Web Bluetooth in this browser (on an iPad: the Bluefy browser)." | The browser cannot talk to Bluetooth. | On an iPad, open CaveCAD in the **Bluefy** browser to connect an instrument. The simulator works anywhere. |
| **Could not connect** | The connection failed. A reason shows in red above. | Try again. |
| **Reconnecting…** | The link dropped. | Wait, or move closer to the instrument. |
| "Waiting for a shot…" | Connected, but no shot yet. | Take a shot on the instrument. |
| "After this station's shot." under **Passage at** | Passage sizes need a station with a shot. | Take the shot, then fill them in. |
| "Splays now" | Close shots are waiting to become a leg or splays. | Press **Splays now** to count them as splays. |
| "N shots from the instrument's memory: not in the notebook." | The instrument kept shots taken while it was not connected, maybe on another day. | Press **Add at** the station to write them there, or **Discard**. |
| Take shot is greyed out | The panel is not ready for a shot. | Check the From and To names, and the instruction line. |

## Related

- [Notebook](notebook)
- [Map](map)
- [Stations](stations)
