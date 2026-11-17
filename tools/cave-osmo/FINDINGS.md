# DJI Osmo Mobile 7: measured behaviour

Running log of what the gimbal actually does, measured with the tools here. Unit in hand: **OM7-403WJA** (Osmo Mobile 7,
not 7P).

## Link (2026-11-16)

- Advertises as `OM7-<serial>` with service `fff0`. Must not be connected to DJI Mimo / the phone.
- GATT: battery (`180f`/`2a19`, notifies), device info (`180a`), `fff0` → `fff3` (write+notify), `fff4` (notify),
  `fff5` (write, write without response), and an extra vendor service `15f1e600-a277-43fc-a484-dd39ef8a9100` →
  `…602` (write), `…603` (notify). The extra service is not in any community project; meaning unknown.
- **macOS bonds with it as a HID keyboard** (the shutter button) on first connect. From then on the Mac reconnects it
  the moment it is disconnected, it stops advertising, and btleplug cannot find it. The Swift probe
  (`run-swift.sh`) takes the system's existing link with `retrieveConnectedPeripherals(withServices: [fff0])`, which
  always works. Any CaveCAD bridge on macOS/iOS needs the same trick.
- DUML checksums as in om-research (CRC-8 init 0x77, CRC-16 init 0xDF0C, reflected): every message the OM7 sent
  parsed clean.

## What it sends unasked

- `04→02 00/f1` once a second, payload all zeros (heartbeat).
- `27→02 04/57`, attributes 0x40 (wants a reply), ~18 per second, payload `00 00 00 00 01 00 00 00`, never changes,
  not even while the gimbal moves. Sender 0x27 is not in any public DUML address list. Unanswered, it did not stop
  the gimbal moving.
- **No angle readings** (`04/05` push, as the Pocket 3 sends) and no answer to `04/02` get-position.
- Battery level on `2a19` (read 29 % = 0x1d).

## What moves it

- **It moves** (Nathan watching, 2026-11-16, Swift probe over the bonded link) with plain om-research messages,
  attributes 0x00, written in 20-byte chunks to `fff5`, without any handshake. Sequence sent: speed yaw 30°/s for 2 s
  (`04/0c`, mode 0x80, refreshed every 100 ms), absolute 0°, absolute 45° yaw (`04/14` mode 0x05, time 20), relative
  −45° (mode 0x04), speed pitch 20°/s for 1.5 s. **Which of these it obeyed is not yet confirmed one by one.**
- The same bytes sent earlier from the Rust probe (unbonded link, right after Forget) got no visible reaction, but
  Nathan was not watching that run, so this is not evidence either way.

### One command at a time (2026-11-16, Nathan watching each run)

| Sent | Result |
|---|---|
| `04/0c` speed, yaw 30°/s for 1.5 s, refreshed every 100 ms | **turned right slowly** |
| `04/14` mode 05, yaw 45° (first run after the mixed run above) | turned right ~45°, **never repeated** |
| `04/14` mode 05, yaw 0° / 45° / −20° (four runs, incl. fresh sequence numbers, incl. after a zero-speed command) | nothing |
| `04/14` mode 04 (relative), yaw −45° | nothing |
| `04/0a` (Pocket 3 "angle set"), yaw −20° in the third slot, flags 01 | nothing |
| `04/0a` yaw −20° in the first slot (payload one byte short by mistake, attributes 0x40) | nothing |

**Conclusion so far: speed control works; no angle command works.** The one 45° swing is unexplained (it may have
been the end of the earlier mixed run, or a follow-mode drift). Sequence-number reuse was ruled out as the cause.
So a sweep must steer by speed and time, and know where the gimbal points from the X1's own compass and clino.

## Measured with the phone in the clamp (2026-11-16, `watch/`)

An iPhone in the clamp runs `watch/index.html` (served over Tailscale https, since iOS gives motion data only to
secure pages) and posts its orientation ~60 times a second to `watch/server.py`; `watch/analyze.py` and
`watch/roll.py` turn that into still positions and laser angles.

- **Timed speed turns are true:** 10°/s × 4.5 s → +44.3°; −45°/s × 3 s → −131.3°; −30°/s × 3 s → −87.6°;
  +30°/s × 5 s → +147.2°; roll 30°/s → 15° per 0.5 s. About 2-3 % short (ramp up/down). Units confirmed: tenths of
  a degree per second.
- **Pan:** about 287° between its stops, i.e. about ±143° from centre. A turn into a stop just stops there.
- **Tilt (camera direction):** +34° up, −24° down from level.
- **Roll** (spin round the camera axis): clean and accurate to −164°, then it snaps back to about −92°. Positive
  roll goes wrong past about +65-70°: it swings round wildly and settles at +90° (looks like its portrait/landscape
  flip). **Plans roll negative only** (`sweep::ROLL_LIMIT` = −150°).
- Drift at rest: about 0.5° a minute in roll (follow mode).
- Once it re-centred itself (tilt 34° → 0°, pan moved ~49°) a few seconds after a new connection; cause unknown.

## X1 mounted laser out of the top (Nathan, 2026-11-16)

At rest the laser points straight up. Pan barely changes the aim, tilt only ±30°; **roll is the ring axis**: rolling
−150° takes the laser from straight up down one wall to about −60°. Pan reaches ±90° from centre, which is enough to
look both ways along the passage, so `sweep::ring` = pan −90°, shoot up, roll down one wall in steps, roll back, pan
180°, the other wall, pan back. The gimbal is set down with its centre facing a wall. Unreached: ~30° wedge straight
down.

## First ring with the real X1 (2026-11-16, home office)

X1 rubber-banded in the clamp, laser out of the top; `osmo-plan -- ring 30 0 flip x1` through the Swift probe;
11 of 11 shots arrived (laser on + measure, ~5 s each including the move).

| Side | X1 inclination after each 30° roll step |
|---|---|
| first | 73.0 → 59.0 → 34.8 → 9.1 → −16.4 → −41.6 |
| second | 64.2 → 37.7 → 10.7 → −16.1 → −42.8 |

- **The X1's weight pulls the gimbal back:** a commanded 30° step lands ~25° (first step of a side ~14°), and the
  straight-up shot reads 73° not 90°. The shots are still true (each carries the X1's own angles); only the spacing
  suffers. Fix: close the loop on the X1's inclination, and balance the X1 in the clamp.
- **The 180° pan fell short:** both fans point at the same side of the room (level shots at azimuth ~223° and
  ~192°, should differ by 180°). Likely a pan stop (gimbal not centred facing a wall at the start) or the weight.
  Fix: check the pan with a level shot's azimuth (X1 compass) and keep turning until it is 180° from side one.
- Laser near vertical: the X1's azimuth swings wildly (334°, 280°, 232°) and means nothing there.

## Weight, step size and speed with the X1 (2026-11-16, X1 inclination as truth)

| Move | Asked | Got |
|---|---|---|
| 15°/s × 2 s | 30° | 29.9°, 27.7° |
| 30°/s × 1 s | 30° | 27.7°, 25.5°, 26.7° |
| 30°/s × 3 s / × 2 s | 90° / 60° | 86.6° / 57.3° |
| 60°/s × 0.5 s | 30° | 23.7°, 22.5° |
| 30°/s × 0.45 s ("10°"), settle 0.5 s, ×8 | — | 11.4-11.7° every step (±0.2°) |
| same, settle 0.2 s, ×8 | — | 10.6-12.3° |
| 30°/s × 0.23 s ("5°"), settle 0.3 s, ×11 | — | 5-8°, mostly ~6° |
| holding still 5 s | 0° | 0.0° (no sag) |

- Longer moves lose ~0.1 s of their start (fast short moves lose most: 60°/s loses ~7°). Short moves at 30°/s run
  slightly LONG (10° → 11.5°). Steps are very repeatable, so a fixed table of move times per step size works, with the
  X1's inclination as the check.
- The weight does not make it sag once still. Rolling the short way past ~40° from vertical snaps back (seen again).
- **Overlap the laser with the move:** send laser on (`36`) as the move starts, measure (`38`) once settled. Shots
  then come every **1.46 s** (the X1's own laser-on-to-reading time is the floor); about half the time the X1 takes
  ~3.4 s instead (its known slow reading). The probe's old fixed 2 s pause after every move is gone.

## Height of the pivot (2026-11-16, floor sweep)

Shots straight down hit the surface the gimbal stands on: for each, (distance + offset) × sin(angle down) = height.
The four deepest (−41° to −59°) fit **height ≈ 0.30 m, offset ≈ −0.02 m** (residuals ≤ 5 mm); with offset 0 they
give 0.31-0.32 m. Too narrow an angle range to pin the offset; check with a tape (surface to roll-motor centre).
Shallower shots hit other things (desk clutter, wall).

## Still unknown

- Whether the gimbal's motors pull the X1's compass, and whether the X1's weight sits well in the clamp.
- Why angle commands are ignored: maybe a mode the Mimo app sets first, or a different command on the OM7 (sniff
  Mimo's traffic with an nRF sniffer to find out).
- Any way to read its angles (try `04/02` with other attributes, the `15f1e6…` service, the `fff3` channel).
- What 0x27 `04/57` wants as a reply.
