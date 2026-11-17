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

## Still unknown

- Per-command: does absolute angle (`04/14` mode 05) work, and what `time` means.
- Angle limits (pitch up/down reach for an LRUD ring) and how close a commanded angle lands.
- Any way to read its angles (try `04/02` with other attributes, the `15f1e6…` service, the `fff3` channel).
- What 0x27 `04/57` wants as a reply.
