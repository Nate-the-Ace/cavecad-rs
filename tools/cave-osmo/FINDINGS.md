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

## First closed-loop ring (2026-11-16, `ring:10`, swift probe)

36 shots in 111 s. Steady stretches ran **1.5 s a shot**; the average was 3.1 s because of one X1 retry and the pan.
- Roll steps steered by the X1's inclination landed 7.5-12.5° for 10° asked (gain learned 0.86-1.11).
- **The long side snaps back past phi ~130** (laser ~40° below level) with the X1 aboard (the phone reached 162°):
  a +10° step went −63°. The ring now stops a side when a step snaps back, and plans to phi 130.
- **The 180° pan failed:** after it the level shot read azimuth 187° against 41° wanted; four corrections flipped
  between 187° and 217° (the pan is stuck at a stop or fighting the weight). Also the laser's inclination changed
  during pure pans (phi 66 → 30 → 107), so pan and roll are not independent with the X1 aboard. Unresolved.

## Recentre (2026-11-17, Nathan watching + X1 readings)

- `04/4c` "reset and set mode" with payload `01 00` / `00 00`, attributes 00 or 40: no recentre.
- `04/0d` "resume" (`b5 2a`): no change (roll 64.0 → 64.0, 36.6 → 36.9, 9.2 → 9.2; distances unchanged).
- When the controlling app disconnects, **pan swings back toward the handle (follow mode) but roll stays**.
- So the app recentres itself: pan/tilt back by its estimate, roll back to vertical by the X1's reading (`aimReset`).
  The trigger double-press recentres AND switches to landscape; no command for it found. Sniffing Mimo would tell.

## DJI Mimo capture (2026-11-17, PacketLogger iOS trace, 230 s, OM7 firmware 01.06.00.02)

How: iPhone with Apple's Bluetooth logging profile, PacketLogger → New iOS Trace, Mimo used normally. Decoded with
the scratch scripts (pklg → ACL → ATT → DUML). One link; Mimo writes only `fff5` (2072 writes), the gimbal answers only
on `fff4` (12,585 notifications). 15,802 messages, all with good checksums once the checksum was fixed (below).

### The checksum (the big one)

**CRC-16 is DJI's standard: seed 0x3692 over the whole message** (header and header checksum included). The community
seed 0xDF0C over the body only is the same thing for a 21-byte message (header `55 15 04 a9`) and wrong for every
other length. So **every non-speed command we sent before had a bad checksum and the gimbal silently dropped it**:
get-position (13 bytes), 0x4C mode/reset (15), 0x0D resume (15), the 0x0A tries. Those "did nothing" results mean
nothing. Speed (0x0C) and 0x14 rotates are 21 bytes, so they were right (0x14 still did nothing for other reasons).
Fixed in `src/lib.rs`, `swift/osmo-probe.swift` and the app (`apps/web/src/lib/instrument/osmo.ts`); the app's
encoder rebuilds Mimo's messages byte for byte (`apps/web/tests/osmo.check.ts`).

### Who is who

- `04` gimbal, `27` handle board (buttons, thumb stick, battery), `02` the app. Mimo also writes to 03, 1c, 1d, 5a, f0.
- **`27→02 04/57` (~18/s) is the handle's thumb stick**: int16 at 0 and 2 run −660…660 (0 at rest, all we ever saw).
  With an app connected the stick is reported to the app instead of moving the gimbal ("the gimbal's joystick did
  nothing" while Mimo was connected).

### Handshake

- The gimbal sends `04→02 00/00` flagged **0x20** (payload `11`); Mimo answers **`02→04 00/00` flagged 0xc0, payload
  `00 11`, same sequence number** (147 times over the session). 60 ms after the first answer the gimbal starts its
  position pushes (`04/05`). We never answered, so we never got them. Now answered (probe and app).
- Mimo's steady traffic: `02→27 00/00 [02 00]` ~2/s, `ee/02` to 04 and 27 ~1/s, polls `04/10` params 0x85 / 0x22 /
  0x69 (answers are setting flags, not angles), `04/50`, `04/68`.

### What the gimbal reports with Mimo connected

| Msg | Rate | What |
|---|---|---|
| `04/05` | 1/s | **position**: int16 tenths at offsets 0, 4, 8, 20, 22 move with the gimbal; byte 6 a mode (134 at rest, 130/66/70/2/6 at times) |
| `04/27` | 10/s | 5 bytes, two changing int16 |
| `04/66` | 5/s | 19 bytes |
| `04/6f` | 4/s | 21 bytes, mostly zero |
| `04/38`, `04/77`, `04/68` | 1/s | constant |
| `03/d7` | 1/s | 13+ bytes, varying (camera/phone link?) |

`04/05` fields at rest: o0 ≈ ±179.9, o4 ≈ 0, o8 ≈ −10.2, o20 ≈ 21, o22 ≈ 94.6. After the rotate button (portrait) o4
≈ 100, o8 ≈ −95. **Which field is which joint is not settled**: next bench test drives one axis at a time with the
phone gyro watching and the pushes logged (`osmo-probe` prints `POS` lines now).

### Commands Mimo sent (all `02→04` unless said)

| Msg | Payload | When / guess |
|---|---|---|
| `04/4c` | `02 00`, `01 00` | mode switch (lock/follow); answered `04/4c` flagged 0x80 |
| **`04/54`** | **`01 dc 05 00 00 00 00 80 00 00 00`**, once a second ×7 | lines up with the gimbal going back to centre (150-157 s): **likely recentre**, 0x05dc = 1500 (ms?) |
| `04/54` | eleven zeros | often, alone: stop / idle? |
| `04/0f` | `82 01 02`, `82 01 06`, `22 01 00`, `8d 04 …` | set parameters |
| `04/12` | 30+ bytes | settings blocks at connect |
| `23/09` | 30+ bytes, 3/s, 81-209 s, floats | a stream (phone attitude? tracking?) |
| `04/72`, `04/6d`, `04/58`, `03/da`, `00/32`, `00/4a`, `00/0c`, `12/07` | — | connect-time queries (`00/32` answers the serial `95DVM25923403WJA`, `12/07` the name) |

No `04/0c` (speed) or `04/14` (angle) from Mimo at all.

## Still unknown

- Whether the gimbal's motors pull the X1's compass, and whether the X1's weight sits well in the clamp.
- Which `04/05` field is which joint; whether its rate can be raised (1/s is slow for steering).
- Whether `04/54 [01 dc 05 …]` is recentre (the app's Reset now tries it first, then checks with the X1).
- Whether 0x14 / 0x0A angle commands work now that the checksum is right (0x0A and short ones were never valid).
- Any way to read its angles (try `04/02` with other attributes, the `15f1e6…` service, the `fff3` channel).
- `04/57` is the handle stick (answered: it wants nothing back; Mimo never replies to it).

## Bench test after the checksum fix (2026-11-17, morning)

Swift probe from the Mac, gimbal in portrait, nothing in the clamp.

- **Handshake works.** Answering the gimbal's 00/00 (flags 0x20) starts 04/05 reports at 1 per second. No faster:
  polling 04/02 (get position) gets no answer; Mimo also saw 1/s.
- **04/05 decoded.** Floats at payload bytes 24..40 are a unit quaternion (w, x, y, z): the clamp's attitude in a
  north-east-down world whose north is wherever the gimbal woke up (no compass, so it drifts slowly but is not upset by
  its own magnet). At rest: (0.707, −0.707, 0.035, −0.016) = rolled −90° (portrait); body +Y is the clamp top, straight
  up. Every speed command moved exactly its own Euler angle of this quaternion:
  yaw 30°/s × 1.5 s → yaw +41.4°; pitch 20°/s × 1 s → pitch +18.5°; roll 30°/s × 1 s → roll +27.2°.
  The int16 tenths at 4, 20, 22 are the pan, tilt and roll **joints** (pan 0 at centre, tilt ~22, roll ~94 at rest in
  portrait). With the clamp on its side one commanded axis moves several joints (a yaw of 42° moved pan +42, tilt −5,
  roll −13), as it must with the roll motor at 90°. o0 and o8 track the attitude too (o0 with pitch, o8 with yaw) and
  are not needed.
- **So: aim by the quaternion, guard limits by the joints.** Laser direction = q · (0, ±1, 0). Relative azimuth comes
  from the gimbal, not the X1's spoiled compass.
- **No angle command moves the OM7.** 04/14 (absolute and relative, attr 0x00 and 0x40): no reply, no move. 04/0a:
  answers `00 00` (ok) once, never moves, for control bytes 00 01 02 03 04 05 07 0f 81 87. Mimo never sends either.
- **04/54 (Mimo's "recentre") is acknowledged (`00`) and does nothing** from an off-centre yaw of 52°. Whatever Mimo
  used it for, it is not recentring here; the trigger double-press stays the only recentre. Reset stays closed-loop.
- **Other pushes seen** (not decoded, not needed): 04/27 ~4/s `28 00 10 00 00`, 04/6f, 04/1c, 04/38, 00/f1, 03/d7
  (a file name `DGS001.DAT`), handle 27→02 05/06 (battery?) and 04/57 (thumb stick, ~18/s).

Control is therefore: timed speed moves (rates land within ~8 %), then read the next report (≤ 1 s) and correct.

## Full range test through the library (2026-11-17)

`run-mac.sh osmo-range 30 [axis|cross+|cross-]` (src/bin/osmo-range.rs): each axis each way, measured by the
gimbal's own report, brought home closed-loop. Clamp in portrait, nothing in it. Joint angles as the gimbal reports
them (pan unwrapped).

| axis | reached (joint) | how it ends | back home within |
|---|---|---|---|
| pan + | +86…+92.5 | hard stop (stall, harmless) | 0.5–1.4° |
| pan − | −202 | hard stop | 1.5° |
| tilt (pitch up) | joint −10 (pitch +32° from rest) | hard stop | 1.8° |
| tilt (pitch down) | joint +30 | already at its stop at rest | — |
| roll − | 63 (−32° from rest) | **flips over** | — |
| roll + | 221 (+125°, laser 36° below level) | **flips over** | — |

- **The roll has no stop: past its window the clamp flips** (swings 80–170° in under a second, nearly knocked the
  gimbal over). Speed makes no difference: creeping at 5°/s it went cleanly to joint 220.7 and flipped at ~221 exactly
  as at 30°/s. It is the gimbal's own switch to the other orientation, not momentum. So the library holds the roll
  inside `OM7_PORTRAIT` (75…205) and `run_to_limit` refuses the roll; `guarded_turn` predicts every joint before each
  step (pan moves the tilt and roll joints too in portrait) and stops at the edge.
- The pan stops are not centred: +86…92 one way, −202 the other (295° total).
- At rest in portrait the tilt joint sits at or near its +30 stop, so "pitch down" from rest has almost no room.
- Homing by timed turn + report correction lands within 0.5–1.8° every time, including from 200° away.
- A target past the roll window has to be reached from the other side: pan round (up to 180°) and roll the long way.

## Aim at a direction (library `Osmo::aim`, 2026-11-17)

`run-mac.sh osmo-aim [dps] [az:inc …]`. `aim::solutions` turns a direction (relative to the gimbal's front at home)
into yaw + roll targets on both roll sides and keeps only those whose predicted joints stay inside `OM7_PORTRAIT`;
`aim` stands the clamp upright before a big pan, pans, rolls, then trims tilt/pan/roll on the gimbal's report.
Default demo run, 30°/s, nothing in the clamp:

| asked az/inc | got | off | s |
|---|---|---|---|
| 90/0 (right wall) | 90.2/0.0 | 0.2° | 8 |
| 0/45 | −2.4/42.5 | 3.5° | 22 |
| −90/0 (left wall: **pans 180, rolls the long way**) | −90.2/−0.3 | 0.3° | 19 |
| −90/−15 | −87.9/−16.3 | 2.5° | 4 |
| 0/90 (up, from pan −180) | 80.9 inc | 9° | 10 |
| −150/10 | refused: out of reach | | |
| 90/−15 | 90.9/−14.3 | 1.2° | 30 |
| 45/20 | 43.8/18.5 | 2.0° | 21 |
| 0/90 (up, from pan −45) | 84.5 inc | 5.5° | 9 |

- Side-wall and below-level targets land within 0.2–2.5°. No roll flip in any run once the guard had 1.5× margin
  and a 2° minimum step (an earlier version trimmed straight-up targets to roll joint 69, 6° from the flip: fixed,
  the trim now stops when it stops improving, and `back_inside` returns the roll into its window first).
- **Straight up is the weak spot**: after a pan the tilt joint sits on a stop (−10 at pan −180), so the clamp top
  can't get back to vertical: 81–85° is the best. Fix ideas: pan back toward centre for near-vertical targets; trim
  tilt before rolling.
- Moves are slow (4–30 s) because every correction waits for the next 1 Hz report and big pans stand the clamp up
  first. Next: skip the upright step when the roll path stays inside the window, trim at higher speed.
- Not yet in the app: apps/web/src/lib/instrument/gimbal-ik.ts still has the old model (both roll sides "long"); port
  `aim::solutions` + the joint windows there.

## DJI's positioning protocol: waypoints (Mimo panorama capture + bench, 2026-11-17)

Captured Mimo doing a 3×3 and a 7-stop panorama (`capture/decode_pklg.py "osmo pano.pklg" --all`), then drove it
from the Mac (`capture/waypoints.py` prints probe steps). **This is how DJI puts the gimbal at an angle; it is the
command we were missing. Every move lands in about 1 s, accurately, with no correction.**

1. `04/4c [01 00]`, `04/0f [22 01 00]`, then **enter**: `04/6b [00 00 00 6d]`.
2. **Upload**: `04/6d [01, L, 2c 00 00 0a, N, N × (u16 60, i16 yaw, i16 roll, i16 pitch in tenths), id]`, L = payload
   length − 2, id any byte (echoed in the status). The gimbal starts for target 0 at once.
3. **Go to target i**: `04/6d [00 06 40 00 i 0a 00 00]` (reply `00 01 01 00` = accepted, `00 00 01 00` = refused).
4. **Status push** `04/6b [01 6d 06 01 S 00 i N id]`: S 0x40 moving, 0x60 arrived, 0x20 aborted (gimbal exits target
   mode and goes home), 0x00 idle. Mimo steps every ~2 s; idle much past 5 s may end target mode.
5. **Exit**: `04/6b [00 01 00 6d]`, `04/6d [01 06 c0 00 00 00 00 00]` (it returns home: send a home target first so
   the exit does not swing).

Targets are **attitudes relative to home**, not joints: the gimbal solves the motors itself (holding yaw while rolling
it swings the pan motor to keep the heading). Measured with the clamp top's direction from the attitude report:

| target yaw/roll | clamp top az / inc | joints pan/tilt/roll |
|---|---|---|
| 0 / 0 | 180 / 74 (home leans 16° back) | 0 / 0 / 90 |
| 0 / 45 | −106 / 43 | 15 / 0 / 45 |
| 0 / 90 | −90 / −0.5 (level) | 92 / 0 / 0 |
| 0 / 110 | −85 / −19 | 144 / 0 / −20 |
| −90 / 90 | 179 / −14 | 93 / 0 / 0 |
| −180 / 90 | 91 / 1 | 95 / 0 / 0 |
| −180 / 110 | 85 / −18 | 37 / 0 / −20 |
| 0 / −30, −60 | | roll joint 120, 150 |
| 0 / −90, −115 | | **roll joint stops at 167** ("arrived"; the gimbal limits itself) |

- **No flip in target mode.** + roll reached roll joint −20 (laser 19° below level); speed commands flipped at 63.
  The − side is capped by the gimbal at joint 167 (≈ 77° of lean). So: + roll is the long side, laser az = yaw − 90°.
- Both walls level and below: left wall = yaw 0, roll 90…110; right wall = yaw −180, roll 90…110.
- Home (0,0,0) is not vertical: the clamp top leans 16° back. Pitch targets of ±16 were refused (aborted, 0x20) right
  after a reboot, with the clamp sagging 16° under the X1; untested since.
- The `60` per target and the header `0a` changed nothing measurable; step byte other than `0a` was refused or
  aborted. Each move takes ~1 s whatever its size, so **to move gently, send small steps** (untested yet with the X1).
- Sometimes the gimbal answers target mode with 0x20 straight away (seen after a reboot); retry after it settles.

### Target mode: limits found running a ring (2026-11-17)

- **The X1's sag is modelled, not commanded away**: v = Ry(sag)·Rz(yaw)·(0, cos ra, sin ra), ra = −90 − roll (the
  sag tilts the whole yaw axis back). Every bench point fits within 2°; straight up is reachable (lean out the sag with
  roll). In app `gimbal-ik.ts` (`targetPointing`, `targetSolutions`).
- **15 targets per list.** 15 run; 16 or more are taken (`00 01 01 00`) but never stepped through.
- **Yaw −200…+95.** −195, −184 run; +120 runs with reply `…80` and status 0x6c (clamped); +176…180 refused (`…90`).
  Same heading 360° lower is used past +95.
- **A new list cannot be sent while one is loaded**: it is half taken (`…10`/`…90`, status jumps to index 1) and the
  next go is refused, target mode ends. Leaving (`04/6b [00 01 00 6d]`) holds position; **entering (`04/6b
  [00 00 00 6d]`) sends the gimbal home** (as does `04/4c [01 00]`). So every list starts from a swing home.
- Ring at 15° spacing, lists 1 and 3 (the ones that ran): every stop **within 0.2–2.5°** of the plan, by the
  gimbal's own attitude report.
- **Home-to-home lists work.** Each list goes out from home (≤5 steps, ≤~37° each), takes a patch of neighbouring
  shots (≤20° apart), comes back home (≤5 steps); between lists leave + enter (enter only, no `04/4c`), at home so the
  re-entry is no move. Medium ring (15 shots, 4 lists) from the Mac: **every stop 0.7–2.5° off, mean 1.6°**, every list
  accepted. Estimated times: ring 15° ~1 min, ring 5° ~3 min, dome 15° ~9 min, dome 5° (1,219 shots, 170 lists) ~79 min.

### Target mode: speed, hand-over, and the X1's weight (2026-11-17, phone and bench)

- **The per-target `60` is the move's duration in 1/100 s.** Timed by status pushes, a 45° roll took: 10 → 1.4 s,
  60 → 1.9–2.2 s, 300 → 3.5–4.2 s, 1000 → 10.4 s. (Judged earlier by the 1 Hz position report, it looked like it did
  nothing.) The app now sizes every move to 15°/s, never under 0.6 s; moves are no longer chopped into steps for
  gentleness, so a 5° dome fits in 98 lists (was 176).
- **On the phone with the X1 aboard, a quick move to roll −61 (short side) overloaded the gimbal** and it dropped the
  list and went home. App roll range now −35…+110 (walls from the long side, turning round when needed; ring reaches
  everything, dome 5° loses ~16% low behind-left).
- **Target mode ends by itself ~9 s after the last command** (two missed X1 shots took 10 s and ended it). While a
  shot is taken the app re-sends "go" to the current target every 3 s.
- **A leave sent while the gimbal is still moving is ignored** (no cleared status), and the next list is then taken as
  a continuation and refused. Wait for the cleared status (S 0x00) after leaving; send it again if it does not come.
- Getting ready (home as a one-target list) left that list loaded: the first scan list was refused. Every list now goes
  in fresh (leave, enter, 2.5 s, upload).
- The first target of a fresh list once sat "moving" past its time; asking for it again got it there.
- Bench, timed moves at 15°/s: the 14-target first list of a ring landed every stop within 3°.
- `PROTOCOL.md` is the clean write-up of all of this for others; the Rust library (`targets` module, `Osmo::targets_*`)
  rebuilds Mimo's captured panorama lists byte for byte (tests).

## Phone camera sweep, first bench pass (2026-11-21)

Hand-free gimbal pass from Capture, phone portrait in the clamp, target mode at ~8 s per move. Sweep plan
(window yaw -195..80, pitch -10..30, 13 targets in one list) ran steps 0-6, then **target 7 was never reached**
(the list stops, the gimbal goes home). The compass followed the pan: home 352°, out to +80 (72°), back past
home to about -185 (163-172°). So the stall is at the far negative end: the pan stop is joint -202 and the tilt
joint sits on its stop there. The window's far end is now -180; a missed target now logs the asked yaw/pitch and
the joints it stopped at. Coverage after the 7 steps: 217 of 396 cells. Fill did not run (pass ended on the miss).
Compass in the clamp read ±13-19° true north, steady through the pass.

## Phone camera sweep, second bench pass (2026-11-21, window -180..80)

Driven from the Mac over Wi-Fi (test commands + live log, no cable). Full pass, no stall: sweep 13/13 in 84 s
(~6.5 s a step), coverage 276/396 cells; fill 11 targets in 61 s, coverage 281/396 at the end (+5 only); total
145 s, saved with no flags. Light 790-1430 lux indoors, tracking normal throughout. `mount` read false for two
steps at the far pan (roll joint leaves 80-112 when the pan couples in portrait) and came back; harmless, but the
mount check must only gate the START, not the pass. Compass in the clamp: ±30° at alignment, ±11° at stop.
Open: fill gained little. Either its yaw sign is wrong (aims away from the holes) or the holes are where the camera
cannot reach/mesh (straight up, under the tripod). Next bench: watch which way the first fill move turns.
