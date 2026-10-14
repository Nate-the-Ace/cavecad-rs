# Cavway X1: how it works, from the open sources (2026-10-14)

Worked out WITHOUT the instrument, from the vendor's own public material. Anything marked **UNVERIFIED** has
two sources that disagree, or only one source, and is checked on the real X1 (see the end).

## Sources (all read in full for this note)

| What | Where | Licence |
|---|---|---|
| Official "Cavway X1 Communication Protocol" v1.0, 2025-03-06 (PDF) | github.com/tswcmpass/CavwayDocs | docs |
| User Manual, Rev. C, 2026-03 (PDF, 8 languages) | github.com/tswcmpass/CavwayDocs | docs |
| TopoDroid fork with X1 driver (`app/src/main/java/com/topodroid/dev/cavway/`, by Siwei Tian) | github.com/tswcmpass/Topodroid | GPL-3.0+ |
| Cavway Assistant for Windows (C#; USB; shot parser `Shot.cs`, `UART.cs`) | github.com/tswcmpass/CavwayAssistant_Win | GPLv3 |
| Nathan's own Web Serial port and X1 emulator (USB only; his reading of the C# code) | github.com/Nate-the-Ace/cavway-assistant-web, cavway-x1-emulator | GPLv3 |

Nothing here was copied from the GPL code: the layout below is facts from the PDF and the manual. Writing our
own decoder from this note keeps the CaveCAD code free of their licence; copying from the Java or C# would not.

## It is not CaveBLE

The first Survey build used the open "CaveBLE" protocol (Shetland Attack Pony). The X1 does not speak it. It uses
the **Nordic UART Service** layout:

| | UUID |
|---|---|
| Service | `6e400001-b5a3-f393-e0a9-e50e24dcca9e` |
| Write (app to X1) | `6e400002-b5a3-f393-e0a9-e50e24dcca9e` |
| Read, notify (X1 to app) | `6e400003-b5a3-f393-e0a9-e50e24dcca9e` |

Good news: this is a plain BLE GATT service, so Web Bluetooth (Chrome, and Bluefy on an iPad) can reach it.
Not classic Bluetooth serial.

## Two ways in

- **Bluetooth LE** (phones, tablets): the service above. The BLE module's own firmware is a separate number
  (0.4.0 on the manual's info page). The X1 has a "Force pair" option, off by default; with it off, do NOT pair in
  the operating system, just scan and connect from the app (the manual says this for Android and TopoDroid).
- **USB-C virtual serial port** (computers): WCH CH343, VID `0x1a86`, PID `0x55D3`, 115200 8N1. Used for
  firmware upgrade, downloading history and calibration coefficients (Cavway Assistant, and Nathan's web port).
  Web Serial, so Chrome or Edge on a laptop; not an iPad.

## Framing (writes to the X1)

BLE: `"data:"` (`64 61 74 61 3a`), one length byte, the payload, then `0d 0a`. The Windows app and Nathan's port use
the same frame on USB; the PDF says USB needs no header or footer (**UNVERIFIED** which is stricter).

Examples (BLE, as written to the Write characteristic):

    laser trigger   64 61 74 61 3a 01 38 0d 0a
    ack             64 61 74 61 3a 01 <flag|0x55> 0d 0a
    read memory     64 61 74 61 3a 04 3d <addrLo> <addrHi> <n> 0d 0a

Single-byte commands (length 1): `30` leave calibration, `31` enter calibration, `32` toggle calibration,
`34` power off, `36` laser on, `37` laser off, `38` laser trigger (take a shot).

## Memory map (read with `3d addrLo addrHi n`; reply is `3d addrLo addrHi n data...`)

| Address | What | Bytes |
|---|---|---|
| `0x0000..0x7FFF` | stored shots, 64 bytes each, 0 = most recent | 64 |
| `0x8000` | device clock (u32, seconds; the Windows app writes local wall-clock as if UTC) | 4 |
| `0x8008` | serial number | 4 |
| `0x9040` | calibration info | 16 |
| `0x9080` | calibration coefficients, both sensor sets | 128 |
| `0xE000` | firmware version (3 bytes: major, minor, patch) | 4 |
| `0xE004` | hardware version (one byte / 10) | 4 |
| `0xC044` | status word (TopoDroid reads it) | |

TopoDroid caps one read at 124 bytes. Writes use `3e` (same layout; the device echoes `3d`). Firmware flashing
(`4b`, `4c`, `4d`) is a USB matter and is not needed for surveying.

## The shot packet (X1 pushes it on the Read characteristic)

"When the storage is not empty, data is automatically transmitted through the BLE read characteristic (notify)."
The X1 stores every shot in memory; each stored shot carries a **transferred** flag (the manual's `*` on the
right in memory mode). The app must acknowledge each packet or the X1 keeps it as unsent and sends it again. A
long press of `>` on the X1 clears the transferred flag of unsent shots (a re-send, or a way out of a stuck
queue).

Layout (little-endian unless noted). Offsets are bytes in the 64-byte packet:

| Offset | Field | Scale |
|---|---|---|
| 0 | type: `0x01` shot, `0x02` calibration | |
| 1 | flag byte (below) | |
| 2, 3, 4 | distance = `b2<<16 \| b4<<8 \| b3` (the order is unusual: high, low, middle) | millimetres |
| 5, 6 | azimuth, u16 | `x * 360 / 65535` degrees (the PDF and TopoDroid use 65536; negligible) |
| 7, 8 | inclination, **signed** i16 | `x * 360 / 65535` degrees |
| 9, 10 | roll, u16 | same |
| 11, 12 | sensor 1 absG | |
| 13, 14 | sensor 1 absM | |
| 15, 16 | sensor 1 dip | same angle scale |
| 17..20 | time, u32 seconds (device local clock) | |
| 21..32 | sensor 1 raw G (x, y, z) then M (x, y, z), i16 | |
| 33..44 | sensor 2 raw G then M, i16 | |
| 45..55 | accuracy error info (0xFF first byte = none) | parser in the PDF |
| 56, 57 | laser signal quality | |
| 58..63 | sensor 2 absG, absM, dip | |

Flag byte, as the Windows app (`Shot.cs`) reads it, which is the code that runs:

- bit 7 = 0: the shot belongs to a **leg** (bit 7 = 1: an ordinary shot, i.e. a splay)
- bit 0 = 0: a calibration shot
- bits 1 to 3: the mark set on the X1 with its M button: `0` feature, `1` ridge, `2` **backsight**, `3` generic
  (the manual lists "No mark" too)

Acknowledge = the flag byte `OR 0x55`, sent framed (TopoDroid: `sendCommand(packet[1] | 0x55)`).

## What the instrument does (manual, Rev. C)

- Two pairs of three-axis gravity and magnetic sensors. The X1 computes azimuth, inclination and roll ON the
  device, from stored calibration coefficients, and compares the two sensor pairs. A disagreement is "ACC ERR"
  (flagged `E` in memory, and detailed in the error bytes). The packet carries the raw readings too.
- Pressing DIST once turns the laser on; again takes the shot. Hold 2 s for a delayed shot.
- **The device decides what a leg is.** "Whenever three or more consecutive close shots are taken" it double
  beeps and shows LEG. So a surveyor shoots a station-to-station leg three times; the X1 marks them as a leg.
  Every other shot is a splay. This is the DistoX way, and it is NOT one-press-one-shot.
- Shot marks (M button while the laser is on): none, feature, ridge, backsight, generic.
- Scan mode (hold DIST+M, or `>`): continuous automatic shots held steady, up to 200, for splay fans (`S` in memory).
- Memory mode: the shots shown newest first with markers: `E` accuracy error, `C` calibration, `S` scan, `*`
  not transferred, `_` has a mark, bold = leg.
- Calibration is done on the device alone (groups of four shots), with the same method as the DistoX2.
- Battery: 1800 mAh, non-magnetic. USB-C charges and carries data. IP67.

## What this changes in the Survey panel

1. **Replace the CaveBLE protocol with this one.** New `lib/instrument/cavway.ts` beside `caveble.ts`: the NUS UUIDs,
   framing, the packet decoder, the ack, the commands. `webble.ts` takes the protocol as a parameter; `link.ts`
   shares the duplicate-frame handling.
2. **The leg/splay switch becomes the device's job.** A reading arrives with `isLeg`, `mark` and `calibration`.
   The session needs: collect consecutive flagged shots, make ONE leg of them (TopoDroid and the DistoX average the
   triple; our rule must be chosen), treat unflagged shots as splays at the station stood at, treat mark
   `backsight` as the backsight, ignore calibration shots (type `0x02`). Nathan's `cavway-assistant-web/src/cavecad.js` already
   merges legs with a guessed tolerance (5 cm, 1.5 degrees); that guess is replaced by what the real packets show.
3. **More to keep per shot, for checks:** accuracy error text, laser quality, the two sensors' agreement. A shot
   with an accuracy error should be flagged in the log, not silently written.
4. **Simulator and tests** emit X1 packets: Nathan's emulator (`cavway-x1-emulator/device/`) already encodes his
   reading of them; reuse its packet builder for the test fixtures rather than writing another.
5. Keep the capture mode: it is how every UNVERIFIED line below gets settled in ten minutes with the real unit.

## UNVERIFIED (settled with the real X1)

1. **Packet size over BLE: 33 or 64 bytes?** The PDF says 64. TopoDroid's X1 driver only accepts a packet of exactly
   33 bytes (offsets 0 to 32: through sensor 1's raw M), and asks for no larger MTU (the X1 negotiates it). The
   decoder should take any length of 33 or more, and the log will say which.
2. **Where the leg flag is.** The PDF's figure puts "LF" at bit 1 and "CF" at bit 0; the Windows app reads leg
   from bit 7 and the mark from bits 1 to 3. The Windows app is the code in use; check with a three-shot leg.
3. **Which packets carry the leg flag:** only the third of three, or all of them? And what a fourth close shot
   does. (Nathan's checklist asks the same.)
4. **Backsight:** a shot marked "Backsight" on the X1 is the reversed leg. Check its azimuth is about 180 degrees
   off the foresight and its clino sign flipped, as the notebook expects.
5. **The ack:** `flag | 0x55` as written in both sources (an odd choice; check the X1 stops re-sending).
6. **TopoDroid's distance bug:** its code reads the top byte shifted by 8, not 16, so a shot past 65.5 m is wrong
   there. The PDF and the Windows app agree with the 16-bit shift. Use that.
7. **Bearings:** magnetic or true. The X1 has no declination setting in the manual; assume magnetic and let the
   trip's declination apply (as for typed readings).
8. **Advertised name** (for a chooser filter): not in any source. Filter on the service UUID instead.
9. Whether pushing starts on its own after connect, or after the first command; and whether a shot taken while the
   app is connected is pushed at once.
10. Web Bluetooth specifics on the iPad (Bluefy): write without response vs with, notification rate, reconnect.
