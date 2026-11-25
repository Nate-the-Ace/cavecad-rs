# X1 firmware 1.1.7-cc3: CaveCAD scan control

Cavway's public 1.1.7, plus three Bluetooth/USB commands and a shot counter, so CaveCAD (and the gimbal sweep) can
run the X1's own scan mode remotely. For Nathan's unit only; it is Cavway's firmware and is not published.

```
python3 tools/x1-patch/build.py      # writes tools/x1-patch/build/CVWY_X1_1.1.7-cc3.bin
```

`build.py` refuses any base but the exact public 1.1.7 (sha256 `18060d77…`) and checks every original instruction
before replacing it. It needs only Xcode's clang. The result flashes like any Cavway file (X1 Flasher ▸ Cavway Release
▸ 1.1.7-cc3, or `x1flash flash … --yes`).

## What changes

| Where | Stock | cc2 |
|---|---|---|
| 0x0800C3DE | `cmp r4,#0x35 / bne exit` (end of the single-byte dispatcher) | `b.w cc_dispatch`, which keeps `35` and adds `39` `3A` `3B` |
| 0x0800D500 | scan exit: `movs r0,#0 / ldr r1,=ticks` | `bl cc_scan_end`: records why the scan ended, then the same two instructions |
| 0x080132C4 | `bl store_record` | `bl cc_store`: in scan mode, writes the scan counter into record bytes 54–55, then stores |
| 0x08004004 | `00 00 00 00` in the version block | `CC 20 02 20` (`movs r0,#0xcc; movs r0,#2`): the "-cc2" marker, readable at memory E000 |
| 0x08017B84 | `"1.1.7"` | `"1.1.7-cc2"`: what the Information page shows |
| 0x0801D200 | erased | 352 bytes of new code (`cc.s`) |

| 0x0800641A | USB sender gate: `ldr r0,=charging / ldr r0,[r0]` | `bl cc_usb_gate`: also send when a USB reply is waiting (cc3) |
| 0x0800C682 | factory defaults: `bl 0x0800C520` | `bl cc_defaults`: sleep 300 s (IdleOff index 4) and beep off, then the same call (cc3) |

Defaults apply on Reset Default and on first boot with blank settings; settings already saved on a unit are kept.

New RAM: 3 bytes at 0x2000BF00, above the stock stack top (0x2000A880). Nothing else is touched.

## Commands

| Byte | Does | Needs |
|---|---|---|
| `39` | Start scan: exactly what DIST+M does, without the keys or the "scan allowed" option | Mode 1 with the laser ready (send `36`, wait ~1.5 s) |
| `3A` | Stop scan, as a press of CLR | Scanning |
| `3B` | Status | — |

Each command answers like a memory read of 0xCC39..0xCC3B (cc1 answered `[cmd, 0, 0, …]`, which the USB sender
drops: it forwards only replies starting `3d`):
`[3d, cmd, cc, 12, mode, laser, count u16, ticks u32, options, result, last end, cc version]`:
result 0 done, 1 wrong mode, 2 laser not ready. Last end: 0 none since power on, 1 CLR, 2 time limit, 3 shot limit,
4 stopped by `3A`. Parsed by `cave_instrument::cavway::parse_scan_status`.

Scan shots carry their number in packet bytes 54–55 (first = 1). `Packet::scan_count` reads it, and it stays `None`
on stock firmware (0xFFFF there). Scan pacing, the 200-shot cap and the time limit are stock 1.1.7 (FIRMWARE.md).

```
x1flash scan status          # over the USB cable
x1flash scan start           # after: x1flash … or 36 over Bluetooth, then wait for the laser
x1flash scan stop
```

## Checked so far

- Every hook and branch in the built image disassembled and read back (targets, bl vs b.w, literal pools).
- Only the six places above differ from stock, plus the header timestamp.
- Full flash rehearsed against the pretend bootloader: 807 packets, UPGRADE SUCCESS.
- **Flashed and tested on the X1, 2026-11-17** (`run-mac.sh x1-scan`, over Bluetooth): status answers (cc2); start
  with the laser off is refused (result 1); `36`, then start: mode 2, laser continuous; shots arrive flagged scan, the
  first flagged sweep start, numbered 1, 2 in bytes 54–55; stop ends it, status then says mode 1, last end Remote.
  Clock set over Bluetooth, read back exact.
- Fixed in cc3: (1) the first status after power-on could show a garbage "last end" (Other(220) seen on cc2): the
  state is now initialised with "none"; (2) USB answered only while charging (stock): now any command that came in
  over USB is answered; (3) defaults: sleep 300 s, beep off.

## Undo

Flash the public 1.1.7 (X1 Flasher ▸ Cavway Release ▸ 1.1.7). The bootloader is untouched, so this always works
(boot mode: hold both arrows, press DIST).

## cc6: the feature picker (2026-11-25)

`1.1.7-cc6` adds an on-device feature list. With the laser on, hold `<` for 2 s: a "Feature" list opens (16 items, five
rows at a time; the order is `CALLOUTS` in `cave-instrument/src/cavway.rs`). `<` `>` scroll, `M` picks (beep), `CLR`
cancels. The next shot carries the pick in packet bytes 54-55: byte 54 = 1-based item, byte 55 = `0xCA`
(`Packet::callout`; `scan_count` ignores those bytes). A pick is used once.

| Where | Stock | cc6 |
|---|---|---|
| 0x0800CFE0 | `push {r4,lr} / sub sp,#0x28` (UI state machine entry) | `b.w cc_ui`: mode `0x40` (the picker) and the `<` long-press opener; any other mode runs the stock code |
| 0x080072BA | `bl 0x08006530` (first call of the per-mode screen function) | `bl cc_draw`: the same call, then draws the list in mode `0x40` |
| 0x0800791A | `bl STORE_RECORD` in the store-N-records loop a **normal shot** uses (record buffer 0x20000C44) | `bl cc_store` (cc3's scan hook at 0x080132C4 only saw scan shots; cc5 missed this, found on the X1 2026-11-25) |

State: CC_STATE +5 cursor, +6 the pick. Tested on the real X1 (2026-11-25): stored record bytes 54-55 = `04 ca` after
picking Stream. Offline: `firmware-lab/tools/test_picker.py` in cavway-x1-emulator (CPU emulator).
