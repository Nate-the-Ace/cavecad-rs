# Cavway X1 quick reference

The controls of the Cavway X1 and how to calibrate it, from the X1's manual (Rev. C, March 2026, firmware 1.1.6). The same as the printed card for the hard case.

## Buttons

| Button | What it does |
| --- | --- |
| **DIST** | Measure (the top button). Also turns the X1 on. |
| **M** | Menu and Enter. While the laser is on: marks the shot. |
| **<** | Left. Opens memory (newer shots). |
| **>** | Right. Opens memory (older shots). |
| **CLR** | Clear and back. Hold to turn off. |

## Power

- **On:** press **DIST**. **Off:** hold **CLR**.
- **Lock and off:** hold **CLR** and press **>**. To unlock, press **M** within 2 seconds of turning on.
- It sleeps after the idle time-off (60 seconds unless changed in Options). CaveCAD reconnects when you wake it.

## Shooting

- **DIST** once turns the laser on; again takes the shot (a beep).
- Hold **DIST** for 2 seconds for a delayed shot.
- **Three close shots in a row make a leg:** a double beep and **LEG** on the screen. Any other shot is a splay.
- **Marks** (laser on): **M** cycles None, Feature, Ridge, **Backsight**, Generic.
- After a shot, **<** and **>** show its errors and its horizontal and vertical parts.

## Scan mode (sweeps)

- Hold **DIST** and **M** for 2 seconds (or **>**, set in Scan options): **SCAN MODE**.
- The laser stays on. Hold still for a moment and it shoots by itself.
- It ends with **CLR**, at the time limit (2 minutes unless changed), or after 200 shots.

## Memory

From measuring, **<** or **>** opens memory, newest on top. **M** opens a shot's detail (four pages: readings, errors, horizontal and vertical, G, M and dip).

| Sign | Means |
| --- | --- |
| **E** | accuracy error |
| **C** | calibration shot |
| **S** | scan shot |
| `*` | not sent yet |
| `_` | has a mark |
| **bold** | a leg |

Holding **>** clears every "not sent" flag. Only do it once the shots are in CaveCAD.

## Menu (M)

- **Calibration**, **Clr. Unsent**, **Options**, **Information**, **Advan. Menu**.
- Options: where distances are measured from (rear, tail, tripod, front, custom), shot delay, backlight, beep, idle time-off.
- Information, second page: the last calibration's errors and dip.

## ACC ERR

The X1's two sets of sensors disagree. First move away from metal and magnets: the phone (MagSafe magnets), a watch, the headlamp, batteries, steel shelving, cars, rebar. If it still happens often with nothing nearby, calibrate.

### The X1's accuracy limits

**M**, then **Advan. Menu**, then **Shot Options** (**M** changes a value):

| Setting | Factory | Loosest |
| --- | --- | --- |
| Err Detect | ON | OFF (no checks) |
| Angle Diff | 0.4° | 1.0° |
| ABS Lim (gravity and magnetic) | 1.0 % | 3.0 % |
| dip Lim | 1.0° | 3.0° |

Looser limits mean fewer ACC ERR, not better shots.

CaveCAD has its own check too: **Accuracy check** in the Capture panel. **Strict** passes on every complaint the X1 makes; **Normal** warns past 2 % gravity, 5 % magnetic, 5° dip or 2° angle; **Relaxed** is twice Normal.

## Calibrate

CaveCAD can guide you: press **Guided calibration** in the Capture panel with the X1 connected.

1. Go somewhere magnetically quiet: no steel, cars or rebar; phone, watch, headlamp and batteries well away.
2. Turn on, then **M**, then **Calibration**.
3. Hold the rear of the X1 fixed at a point A, with the laser on a target B at least 5 m away.
4. **DIST** to shoot, **roll the X1 90°** about the laser, shoot again. **Four shots** make a group: a double beep when it is complete.
5. Choose a new direction and repeat. Cover **all directions**: level all round, steeply up, steeply down, and in between.
6. The two circles on the X1 (the upper and lower half) darken as directions are covered; **x** marks a finished group, **+** the current one. Fill both: **14 or more groups** (25 at most).
7. A bad shot in a group: **>** resets that group.
8. After 8 or more groups, **M** computes. Read the report for each sensor set.
9. **M** applies the calibration; **CLR** discards it, and you can take more groups.

Rule of thumb: an average error of **0.3° or less** is good (the manual's example: average 0.25°, maximum 1.28°). Much worse: discard it, look for metal nearby, take more groups.

Calibrating by hand is recommended; a jig must keep the laser dot still while the X1 rolls. Firmware upgrades erase the calibration: save the coefficients first with Cavway Assistant.

## With CaveCAD

- **Connect** in the Capture panel (Survey tab on a computer), and pick **CavwayX1-…**.
- **Single shot** and **Station to station**: the first press turns the laser on to aim, the second shoots (one shot, or three for the leg).
- Keep the phone at least 30 cm from the X1 while shooting.

## Firmware boot mode

With the X1 off and on a USB cable to a computer: hold **<** and **>** and press **DIST**. Then Cavway Assistant, **Firmware**.
