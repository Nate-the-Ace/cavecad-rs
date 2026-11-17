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

## Still unknown

- Whether the gimbal's motors pull the X1's compass, and whether the X1's weight sits well in the clamp.
- Why angle commands are ignored: maybe a mode the Mimo app sets first, or a different command on the OM7 (sniff
  Mimo's traffic with an nRF sniffer to find out).
- Any way to read its angles (try `04/02` with other attributes, the `15f1e6…` service, the `fff3` channel).
- What 0x27 `04/57` wants as a reply.
