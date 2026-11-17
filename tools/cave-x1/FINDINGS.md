# Cavway X1: what we have measured

A running log of what the real X1 does, found with this library (and the app's debug log). Each entry says how it was
found, so it can be re-run. The wire format itself (packets, flags, memory map) is in `docs/CAVWAY-X1.md`; this file is
about behaviour and timing. Add to it whenever a test teaches something; put anything the library should rely on into
`src/lib.rs` too (a constant, a helper, or the refused-commands list).

Device: CavwayX1-0312, firmware 1.1.6. Bench: MacBook, X1 lying still, pointed at a wall about 1.5 m away.

## Laser and remote shots (2026-11-17, `x1-timing`, `x1-overlap`)

| Finding | Evidence |
|---|---|
| The laser turns off after every remote shot. | `38` straight after a reading: 0 of 5 answered. |
| `38` with the laser off does nothing (no shot, no laser). | 0 of 5. |
| From laser on (`36`), a reading takes 1.40 to 1.50 s, however soon `38` follows (0 to 1000 ms). | 30 of 30. |
| Laser on and settled for 1.5 s, then `38`: usually 0.36 to 0.39 s; sometimes 2.2 s; once no answer. | 4 of 5 answered. |
| A laser on sent 50 or 150 ms after `38`, while it measures, loses the next shot. At 300 ms it works, with no gain. | 8 of 8 lost; 4 of 4 at 300 ms. |
| `36` and `38` back to back after a reading: 1.50 to 1.56 s. | 4 of 4. |
| Three remote shots (a leg) take about 3.3 s at best; the X1 flags the third as LEG. | 5 of 5 legs, 3.32 to 3.47 s. |

So: fire a later shot as `36` then `38` once the previous reading has arrived; wait at least 3 s before deciding a shot is
lost (the 2.2 s case); never send `36` while a shot is measuring.

## In the app (2026-11-16/17, iPhone debug log)

| Finding | Evidence |
|---|---|
| Station to station works: three remote shots, CaveCAD writes the leg. | Leg A17 to A18, 3.7 s with 250 + 700 ms waits; 3.35 s with 0 + 250 ms. |
| A retry fired at 2 s doubled a shot (the first reading arrived late). | 00:47:19, two readings 0.2 s apart. Retry now at 3 s. |
| After a sleep the X1 comes back under a new Bluetooth identity; it is found again by name. | Reconnect fix, commit d0ee908. |
| The X1 sends nothing stored on connect when everything is acknowledged. | 0 stored on every bench connect. |

## Commands (2026-11-17, `x1-probe`)

| Byte | What | Status |
|---|---|---|
| `30` `31` `32` | calibration mode leave / enter / toggle | documented (Cavway), refused by the library |
| `34` | power off | documented, refused |
| `35` | nothing seen (laser off, 5 s listening) | probed |
| `36` `37` | laser on / off | documented, used |
| `38` | measure (needs the laser on) | documented, used |
| `39` + addr + 4 bytes | write 4 bytes to memory (TopoDroid) | refused. Sent once alone by mistake: no answer, X1 measured normally after (same dip and accuracy complaints as before) |
| `3a` `3b` `3c` | firmware read / write, hardware signature (bootloader) | refused |
| `3d` + addr + n | read memory | documented, not yet in the library |
| `3e` + addr + n + data | write memory (TopoDroid) | refused |
| `3f` | nothing seen | probed |

No remote scan start was found. Asked of Cavway in `docs/cavway-feature-request.md` (item 1).

## Still to find out

- A command that starts scan mode, or a status word (memory `0xC044`?) that says when a scan ends.
- Why a settled laser sometimes takes 2.2 s, or no answer: battery, distance, target?
- Whether the 1.45 s warm-up changes with distance or target colour (the bench was 1.5 m to a pale wall).
- Scan-mode rate (about 1.5 shots a second from the app log) measured here.
