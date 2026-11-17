# X1 Flasher

A small Mac app that flashes Cavway X1 firmware over the USB cable, with the recovery card one click away.

```
tools/x1-flasher/build-app.sh      # builds and installs ~/Applications/X1 Flasher.app
```

## How it is put together

| Part | Where | Job |
|---|---|---|
| Protocol | `crates/cave-instrument/src/cavway_flash.rs` | The update file, the bootloader's K/L/M steps, a sans-IO `Flasher`, and `SimBootloader` (a pretend X1 built from the bootloader disassembly). Pure Rust, so CaveCAD can reuse it over Bluetooth. |
| Command line | `tools/cave-x1/src/bin/x1flash.rs` | The USB serial port (115200, raw termios) around the protocol. |
| Window | `tools/x1-flasher/Sources/X1Flasher` | SwiftUI. Runs the bundled `x1flash --json …` and shows what it says. |
| Recovery card | `tools/cave-x1/recovery.html` | Copied into the app; Help ▸ X1 Recovery Card (⇧⌘R) or the life-ring button. |

## `x1flash`

```
x1flash ports                       # the X1's USB serial ports
x1flash info                        # firmware, bootloader, serial; or "boot mode"
x1flash inspect FILE.bin            # what an update file is
x1flash flash FILE.bin --simulate   # rehearse against a pretend X1 (nothing sent)
x1flash flash FILE.bin --yes        # flash (the X1 must be in boot mode)
x1flash off                         # power off (also leaves boot mode)
```

Add `--json` for one JSON object per line. The binary is inside the app at
`X1 Flasher.app/Contents/MacOS/x1flash`.

## Safety rules it keeps

- It only flashes an X1 that answered as **boot mode** (bootloader `J` → `4A 0A 00`). A running X1 is refused with
  the button instructions.
- It refuses files without the Cavway header, files that don't start like X1 app code, and anything bigger than the
  app area (0x3BF00 bytes).
- It never sends the final install step (`M`) unless every packet was confirmed with the right CRC. Any earlier
  failure leaves the X1's firmware untouched, and the app says so.
- A damaged packet (status 2) or a lost answer is resent up to 3 times. A failed flash write (status 1) stops at once.
- Flashing erases the X1's stored shots. The confirmation sheet says so.
- `x1flash` takes the USB port exclusively (TIOCEXCL). Learned the hard way, 2026-11-17: with the app's 3-second
  check opening the same port during a command-line flash, one answer in ~30 vanished.
- The bootloader adds a CRC to its total for every packet it writes, so a packet resent after a lost answer counts
  twice. The flasher remembers which resends are unsure and offers each possible total at the end; a wrong total is
  refused before anything is erased. More than 4 unsure resends: it stops and says start again.

## Tested

- `cargo test -p cave-instrument cavway_flash`: 11 tests, including a full flash, resends, a packet that keeps
  failing (old firmware intact), and the real 1.1.4 file.
- `x1flash flash CVWY_X1_Firm_1_1_4.bin --simulate`: 743 packets, UPGRADE SUCCESS.
- On the real X1 (2026-11-17): `info` in boot mode answered bootloader 0x0A over USB.
- **Not yet done on hardware: a real flash.**
