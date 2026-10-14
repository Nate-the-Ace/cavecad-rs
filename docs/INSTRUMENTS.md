# Supported instruments: how each one talks (2026-10-14)

Worked out WITHOUT any of the instruments, from their open repositories and TopoDroid. One **Connect** button in the
Survey tab serves all of them (`lib/instrument/webble.ts`); this note says what it speaks and what is not yet checked
on a real unit. The Cavway X1 has its own note, `docs/CAVWAY-X1.md`.

## Sources

| What | Where | Licence |
|---|---|---|
| BricQt: a Qt client for the BRIC (Philip Schuchardt) | github.com/vpicaver/BricQt | MIT |
| CircuitPython CaveBLE: the SAP6's protocol (Phil Underwood) | github.com/furbrain/CircuitPython_CaveBLE | MIT |
| STIC: the SAP6's firmware | github.com/furbrain/STIC | see repo |
| SAP5: the SAP5's hardware and firmware | github.com/furbrain/SAP5 | see repo |
| TopoDroid (`dev/bric`, `dev/sap`, `dev/Device.java`, `TopoDroidProtocol.java`) | github.com/marcocorvi/topodroid, and the Cavway fork github.com/tswcmpass/Topodroid | GPL-3.0+ |

Nothing was copied from the GPL code; the layouts below are facts about the instruments. BricQt and CaveBLE are MIT.

## Telling them apart

The Cavway X1 and the SAP5 offer the very same Bluetooth service (Nordic UART, `6e400001-b5a3-f393-e0a9-e50e24dcca9e`),
so the service cannot say which is which. TopoDroid goes by the **name the instrument advertises**, and so does Connect
(`lib/instrument/identify.ts`):

| Advertised name starts with | Instrument |
|---|---|
| `CavwayX1-` | Cavway X1 |
| `SAP6_`, `SAP_` | SAP6 |
| `SAP5_`, `Shetland_` | SAP5 |
| `BRIC4_`, `BRIC5-`, `BRIC5_`, `BRIC-` | BRIC4 or BRIC5 |
| `DistoXBLE-` | recognised, not supported yet |

The browser's chooser is offered exactly these names (and the CaveBLE and 0x58D0 services). A name that is not known:
the CaveBLE service alone means a SAP6, the 0x58D0 service a BRIC; Nordic UART alone is told by the packets (8 bytes: a
SAP5; 33 or more: an X1). "Show every Bluetooth device" (the Survey panel's capture section) lists everything for an
instrument with a name this does not know.

**Not possible in a browser:** the original DistoX and DistoX2 (classic Bluetooth serial). No Web Bluetooth page can
reach them; they would need a native app.

All of these send single shots; none says "this is a leg" except the X1's flag. So for every instrument three close
shots become one leg (`lib/instrument/legs.ts`), as in TopoDroid, and any other shot is a splay.

## SAP6 (Shetland Attack Pony 6): CaveBLE

Service `137c4435-8a64-4bcb-93f1-3792c6bdc965`: name `…966` (read: the string "SAP6"), command `…967` (write, no
response), leg `…968` (notify). A frame is 17 bytes, little-endian: sequence bit, then float32 azimuth, inclination, roll
(degrees) and distance (metres). Acknowledge each frame by writing `0x55 + sequence bit`; the pony resends until it hears.
Commands (single bytes): `0x38` take a shot, `0x36` laser on, `0x37` laser off, `0x34` power off, `0x31` and `0x30` start
and stop calibration. The firmware sends each reading as it is taken (`measure.py`: `send_data` per reading), so there is
no leg flag. Decoder: `caveble.ts`. Confirmed against both the CaveBLE source and TopoDroid.

## SAP5 (Shetland Attack Pony 5): Nordic UART, DistoX packets

Service `6e400001-…` with the characteristics the other way round to the X1: the SAP5 notifies on `6e400002-…` and is
written to on `6e400003-…`. Each notification is one 8-byte DistoX data packet: byte 0 = type (`0x01`, low six bits) |
`0x40` distance bit 16 | `0x80` sequence bit; bytes 1 to 2 distance in millimetres (low, high); bytes 3 to 4 azimuth,
`x * 180 / 32768` degrees; bytes 5 to 6 inclination, signed, `x * 90 / 16384` degrees; byte 7 roll, `x * 180 / 128`
degrees. Nothing is acknowledged. **Firmware fault** (reported by its author, 2023): bit 16 is set once a distance
passes 32.768 m, not 65.536 m; the packet is corrected by clearing bit 16 when byte 2's top bit is set, as TopoDroid does
by default. Decoder: `sap5.ts`. **UNVERIFIED:** whether the SAP5 can be told to fire from the screen (no command found, so
the button does nothing for it), and the corrected distance past 32.768 m.

## BRIC4 and BRIC5

Services (16-bit uuids on the standard base `0000xxxx-0000-1000-8000-00805f9b34fb`):

| uuid | what | how |
|---|---|---|
| `58D0` | measurement service | |
| `58D1` | primary | indicate, 20 bytes |
| `58D2` | meta | indicate, 19 bytes |
| `58D3` | errors | indicate, 18 bytes |
| `58D4` | last time | read and write, 20 bytes |
| `58E0` / `58E1` | control service / characteristic | read and write ASCII commands |

A shot is three indications in order. **Primary**: year (u16), month, day, hour, minute, second, hundredths (u8 each),
then float32 distance (metres), azimuth, inclination (degrees). **Meta**: u32 running index, float32 dip, roll,
temperature, u16 samples averaged, u8 type (0 a regular shot, 1 a scan shot). **Errors**: two entries of u8 code and two
float32 values: 1 to 4 accelerometer or magnetometer too high, 5 and 6 the two sensors disagree, 7 target moved, 8 and 9
laser signal weak or strong, 10 pattern error, 11 timeout, 12 laser error, 13 wrong message, 14 clino, 15 azimuth.
The shot is complete when the errors arrive; if they never do it is written after 400 ms. A primary identical to the last is a repeat; a
shot with no length is ignored; a jump in the running index means shots were missed (logged). Commands, ASCII to
`58E1` written with a response: `shot`, `scan`, `laser`, `power off`, `clear memory`. Decoder: `bric.ts`; link
`BricLink`. BRIC4 and BRIC5 are treated as one protocol (TopoDroid does).

**UNVERIFIED:** that a browser's `startNotifications` enables the indications these characteristics use (the Web
Bluetooth API says it does); that `shot` alone fires the laser (TopoDroid has `laser` as a separate command); whether the
instrument pushes shots stored while the page was not connected: **not built**, the `LAST_TIME` write that asks for a
download from a given time is decoded (`lastTimeBytes`) but not used, so shots taken while disconnected are not fetched
and `clear memory` is not offered; and what error codes 14 and 15 are (TopoDroid reads their value as a clino or azimuth correction).

## The simulator: each device, with its quirks

The Survey tab's Simulator group picks a device (Cavway X1, SAP6, SAP5, BRIC5) and a quirk. The simulated device writes
its OWN wire format (the same encoders the tests use) and hands it to that device's real link and decoder, so framing,
acknowledgement, repeat handling and decoding all run as for a real unit; the log (capture section) shows the frames. Picking
another device or quirk while it runs starts a new one that carries on from the station the surveyor is at. A quirk should
either change nothing about the survey (the logic copes) or show exactly what it is meant to:

| Device | Quirk | Should show |
|---|---|---|
| X1 | leg flag on every shot / no leg flag | the same survey: a leg is found by closeness, with or without the flag |
| X1 | 33-byte packets | the same survey |
| X1 | lost acks | the same survey; every repeat is acknowledged, written once |
| X1 | calibration shots mixed in | the same survey; acknowledged and left out, the log says so |
| X1 | accuracy errors | the same survey, some shots with an "accuracy error" warning |
| SAP6 | lost acks | the same survey; each repeat acknowledged again (0x55 or 0x56) |
| SAP5 | long shots (firmware bit-16 fault) | legs past 32.768 m still read their true length |
| SAP5 | repeated packets | the same survey |
| BRIC5 | no error indication | the same survey (a shot waits for the next, or 400 ms) |
| BRIC5 | missed shots | the same survey; the log says shots were missed |
| BRIC5 | scan shots | the same survey |
| BRIC5 | zero-length shots | the same survey; the log says one was ignored |
| BRIC5 | instrument errors | the same survey, some shots with "the target moved" |
| BRIC5 | repeated indications | the same survey |

`tests/devsim.check.ts` runs every one and also checks that all four devices write the same survey (distances to 1 cm,
angles to 0.05 degrees). **What it found already:** only the X1 can mark a leg as a backsight; SAP5, SAP6 and BRIC cannot,
so after a leg with backsights on, the next leg is taken as its backsight if it comes back along the one before (within 25
degrees and about as long: `looksLikeBack`), and as the next leg if it does not (the backsight was skipped). A real
switchback in a cave that happens to come back that way, straight after a leg with backsights on, would be taken as a backsight:
the usual backsight warnings (compass, clino and tape disagreeing) flag it.

## How it is tested

`tests/devsim.check.ts` (every device and quirk, and all four agreeing), `tests/instruments.check.ts` (BRIC and SAP5 bytes written out by hand, the links, identification), `tests/webble.check.ts`
(the whole connect flow for each instrument against a stand-in for the Web Bluetooth API, including an unknown name,
unsupported and cancelled), `tests/caveble.check.ts`, `tests/cavway.check.ts`. No instrument has been connected yet.
