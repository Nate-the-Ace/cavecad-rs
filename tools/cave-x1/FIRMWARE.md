# Cavway X1 firmware: a map for adding Bluetooth commands

Read-only reverse engineering, 2026-11-17. Nothing was sent to a device. Images studied (from
`cavway-upstream/CavwayAssist/Resources`):

| File | What it is |
|---|---|
| `CVWY_X1_Firm_1_1_4.bin` | Update image, version 1.1.4, header timestamp 2026-01-22 15:51:59 UTC. 256-byte header, then the app. |
| `CVWY_X1.bin` | Full flash image of an older build (1.0, "2024/11/15" per the commented-out flasher code): bootloader at 0, app at 0x4000. |

Nathan's unit runs **1.1.7** (read off the device 2026-11-17, see "Read from the real X1" below; FINDINGS said 1.1.6),
which we do not have. Every address below is in **1.1.4**. The layout (bootloader,
flash sectors, protocol) is shared by both images, so it almost certainly holds for 1.1.6 too, but code addresses won't.

Tools: capstone 5 (Thumb/M-class) in a scratch venv, plus a linear-sweep listing. No Ghidra.

## Verdict

**Adding Bluetooth commands is feasible, and a bad patch can be recovered, with one big catch: 1.1.4 has no scan mode.**

- The bootloader (flash sector 0) is never erased by an update and always comes up first. Hold the two arrow keys
  while powering on and it waits for a new image over the same Bluetooth link. Stock 1.1.4 can always be put back.
- The command dispatcher is a single function with a clean 4-byte hook point and many unused command bytes.
  There is 146 KB of free flash in the app region and plenty of spare RAM.
- Nothing checks the image except per-packet CRC-16 and a sum. There is no signature.
- **But 1.1.4 has no scan mode.** No string mentions it, and no code ever clears bit 6 of the flag byte. The DIST+M
  scan that Nathan's 1.1.6 does was added after 1.1.4. A patch built on 1.1.4 would lose scan mode, plus whatever
  else 1.1.5 and 1.1.6 added. We'd have to write our own continuous-measure mode, which looks easy (see 5).
  Or get the 1.1.6 `.bin` and patch that instead. Either way: **get a copy of 1.1.6 before flashing anything**.
  The X1 can't give it to us over Bluetooth (see "Backing up the firmware").

## 1. Memory map

MCU: **STM32F4-class**. Evidence: RCC at 0x40023800, FLASH at 0x40023C00 with KEYR 0x45670123/0xCDEF89AB, and erase
by sector number in StdPeriph form (`sector << 3`). The app also uses UART7/UART8 (0x40007800/0x40007C00) and
TIM6/7. That points to an F427/F429/F437/F439 or a GD32F4 clone. The exact part is unconfirmed. Flash used is 512 KB.

| Flash | Sector | Contents |
|---|---|---|
| 0x08000000–0x08003FFF | 0 (16 KB) | **Bootloader** (0x1400 bytes used). Bootloader version word at 0x08000040 = `0a 00`. |
| 0x08004000–0x0803FFFF | 1–5 (240 KB) | **App.** 1.1.4 occupies 0x08004000–0x0801B317. Its first 0x20 bytes are the version block `01 01 04 00 00…`, which run as harmless Thumb instructions. Real code starts at 0x08004020 (`ldr sp,=0x2000A840; bl …; ldr r0,=0x08013C31; bx r0`, i.e. ARMCC startup into `__main`). |
| 0x0803FF00 | 5 | A 2-byte "first boot after update" marker. If it reads 0xFFFF, the app does a one-time init and writes it (0x08008E34). |
| 0x08040000–0x0807FFFF | 6–7 (2 × 128 KB) | **Shot memory**: a ring of 4096 × 64-byte records (0x08008048: index → `0x08040000 + (i/2048)·0x20000 + (i%2048)·64`). **Also the update staging area.** |

RAM: the init data is copied to 0x20000000 (0x1D4 bytes) and the zero-init area runs to 0x2000A840, with the stack
inside it. Everything above 0x2000A840 is unused.

The update image layout: header bytes 0–255 are the ID `11 23 55 6e 7c ef 6d 5b`, the timestamp (big-endian, bytes
8–11) and the version (bytes 12–14). The header is never sent to the device. **Byte 256 of the file goes to
0x08004000.** Proof: the ARMCC region table at file offset 0x173CC points at 0x0801B2EC, its own address + 0x20.

### How an update runs (bootloader, 0x08000060)

The bootloader reads `data:` + length + payload frames from the Bluetooth UART, the same framing CavwayAssist uses:

| Cmd | Bootloader action |
|---|---|
| `K` (0x4B) | Erases **sectors 6 and 7** (0x08040000–0x0807FFFF). Replies `4B 01`. |
| `L` idx16 + 128 B + CRC16 | Checks CRC-16/Modbus, then writes to **0x08040000 + idx·128** (refused at 0x08080000 or above). Replies `4C idx 00/01/02 crc`. |
| `M` sum32 | If the sum matches the sum of packet CRCs: erases **sectors 1–5**, copies staging → 0x08004000 in 1 KB steps, shows "UPGRADE SUCCESS". Otherwise "UPGRADE FAIL", and the app is left alone. Replies `4D 00/01`. |
| `J` (0x4A) | Replies `4A` + 2 bytes from 0x08000040 (bootloader version). |
| `4` (0x34) | Power off (clears the PB10 power latch). |

So:

- **Sector 0 (the bootloader) is never erased or written by an update.** A broken app can always be overwritten.
- **A failed or interrupted transfer leaves the old app in place.** The app is only erased after the sum check passes.
  The only risky moment is a power loss during the M copy (a second or two). Even then the bootloader survives, and the
  update can be run again.
- **Every update wipes shot memory** (sectors 6–7), so unsent shots are lost. Acknowledge or download them first.
  1.1.4 keeps no calibration or settings in flash outside these areas: the only other flash write is the 0x0803FF00
  marker. Calibration and settings therefore probably live as records in the shot ring too, or in RAM only. **Unconfirmed:
  check whether calibration survives an update.**

### Entering boot mode

The bootloader's reset path (0x08000048) does: power latch on (PB10 high, 0x080003FC) → clocks → `0x080002E8`, which
reads GPIOC IDR (0x40020810). **PC1 and PC2 both low (pressed) → boot mode**. Otherwise it calls 0x08004000 (the app).

The app's key poll (0x08005AE8) maps the keys:

| Pin | Key struct | Key | Evidence |
|---|---|---|---|
| PA8 (active high) | 0x20001250 | DIST | idle + press → laser on (0x0800C488); BLE `36`/`38` press it |
| PC0 | 0x2000125A | M | idle + press → mode 3 (menu) |
| PC1 | 0x20001264 | `<` / `>` | idle + press → mode 8 (shot browser, "< >:Prev/Next shot") |
| PC2 | 0x2000126E | `>` / `<` | same |
| PC3 | 0x20001278 | CLR | BLE `37` presses it; long press → power off |

**Manual boot mode: hold both arrow keys and power on with DIST** (inferred: DIST on PA8 is most likely the power
switch, since the bootloader latches power the moment it starts). The screen shows "BOOT MODE".

**Over Bluetooth: no way in.** The app ignores `4B`, and the bootloader only checks the buttons. CavwayAssist's
`sendFWUpdateStart` only works once the device is already in boot mode.

## 2. The Bluetooth command dispatcher

Bluetooth is a serial module on **USART2**, with a DMA1 Stream 5 circular receive buffer of 256 bytes at 0x200006D4.
The module talks AT commands (`AT+POWER=6`, `AT+STATUS=1`, `AT+RESTART`, `AT+EXIT`, `AT+VERSION`).

- **Line parser** `0x0800531C` returns event codes: 1 `OK`, 2 `DEVICE START`, 3 `CONNECTED`, 4 `DISCONNECTED`,
  7 `data:` frame (payload at 0x20001366, length at 0x200000F0), 8 `BONDING FAILED`, 9 `ERROR`, 10 version.
- **Event handler** `0x08005654` (switch at 0x08005666). Case 7 calls the dispatcher:
  `0x0800578C: bl 0x0800B7FC(1, 0x20001366, len, 0x20001460)`.
- **Dispatcher `0x0800B7FC`** (src, payload, len, reply buffer):

| Payload | Code | Action |
|---|---|---|
| len 1, == byte at 0x2000010C | 0x0800B816 | **Shot ACK** (flag \| 0x55): marks the record sent, sends the next (0x08004DAC) |
| `30` `31` `32` | 0x0800B83C… | `0x080074F8(0/1/2)`: calibration mode off / on / toggle (mode 0x31) |
| `33` | 0x0800B860 | **Accepted, does nothing** (only reloads the idle timer) |
| `34` | 0x0800B864 | Power off (event 9, 0x20000080 = 1) |
| `36` | 0x0800B876 | If mode is 0, 2 or 0x31: **press DIST** (0x20001251 = 1) → laser on |
| `37` | 0x0800B898 | If mode is 1: **press CLR** (0x20001279 = 1) → laser off |
| `38`, and `35` too | 0x0800B8AA | If mode is 1 or 0x31: **press DIST** → measure. (`35` is an undocumented alias of `38`. The probe saw nothing because the laser was off.) |
| len 4: `3D` a16 n | 0x0800B8CA | Read memory → reply `3D a16 n data` |
| len ≥ 8: `3E` a16 n data | 0x0800B8DA | Write via 0x080118F4, then read back |
| anything else | → 0x0800B96A | Ignored |

**Remote commands are fake key presses.** They set the same event byte the button poll sets, and the main state
machine (0x0800C3D4) can't tell them apart. **Every path, including ignored bytes, ends with `bl 0x0800BBA8`, which
reloads the idle timer** (see 4).

**Unused single bytes in 1.1.4:** 0x00–0x2F, 0x39–0x3C, 0x3F–0xFF (except whatever the current ACK byte is). `39`,
`3A`, `3B` and `3C` are DistoX/TopoDroid codes that this firmware ignores.

**Memory-read map** (0x08007F74, 16-bit address → pointer):

| Address | Maps to |
|---|---|
| < 0x8000 | Shot record, counting back from the newest (ring at 0x08040000) |
| 0x8000 | Clock, u32 seconds (0x2000003C, refreshed first) |
| 0x8008 | 0x08000044 (bootloader area) |
| 0x9000–0x90FF | Computed block via 0x0800B506 |
| 0xE000 | **0x08004000: the app's version block** (read `3D 00 E0 04` → `01 01 04 00`) |
| 0xE004 | 0x08000040: bootloader version |
| 0xF000 | Echo of the written data |

FINDINGS asks about 0xC044. In 1.1.4 it maps to nothing, so it may be new in 1.1.6.

## 3. Scan mode

**Not present in 1.1.4.** What 1.1.4 does have:

- **Modes** (byte at 0x20000060, handlers in the 0x0800C3D4 jump table): 0 idle, 1 laser on, 2 result shown,
  3 menu, 8 shot browser, 0x31 calibration, 0x33 clock set, 0x35 confirm dialogs, plus others.
- **Shot pacing in mode 1** (0x0800C4EC): one tick is 25 ms. 0x200000BA counts ticks since the laser came on.
  A DIST press starts measuring at `40 − ticks` (**at least 1.0 s of laser warm-up**). That is where the 1.4–1.5 s
  for `36` then `38` comes from: 1.0 s warm-up plus about 0.4 s to measure.
  A long DIST press gives a delayed shot (1000 + 40·n ticks, with a beep every second).
  **The laser times out after 1800 ticks (45 s)** or on CLR (0x0800C672). When a reading finishes, the laser goes
  off and the mode becomes 2. That is why "the laser turns off after every remote shot".
- **Calibration** (0x0800AAF8) is the only automatic multi-shot routine. It fires when the instrument is held still
  (0x08009928, threshold 200), stores shots in groups of 4, and stops after 25.

**For 1.1.6, still unknown:** how DIST+M enters scan, how it paces, and how it ends (CLR, time limit, 200 shots).
We need the 1.1.6 image to answer this.

## 4. Idle power-off

- The counter is `0x2000008C`, in 25 ms ticks. It counts down in the per-tick UI routine (0x0800D55E). At 0, or on a
  CLR long press, the X1 powers off (event 9, 0x20000080 = 1).
- **Reload: `0x0800BBA8`** sets it to `IdleOff[setting] · 1000 / 25`. The table is at 0x08015E06: **60, 120, 180,
  240, 300 s**. The setting is at 0x20001004+4. If the result is 0, it uses 2400 ticks (60 s).
- **What resets it:** any key press, short or long (key debounce 0x08013BC4), changing the IdleOff option
  (0x0800D2DE), and **every `data:` frame received over Bluetooth**, including ACKs and unknown bytes (0x0800B96A).
- **So a keep-awake already exists:** send `33` (a documented no-op) every 30–50 s. If 1.1.6 kept this, no firmware
  change is needed for staying awake. Easy to check on the bench: idle at 60 s, send `33` every 30 s, and see whether
  it stays on past 2 minutes.

## 5. Feasibility of adding commands

**Hook.** The dispatcher's last single-byte test is

```
0800b8ae: 352c   cmp r4, #0x35
0800b8b0: 5bd1   bne #0x800b96a
```

Replace those 4 bytes with `b.w patch`. The patch redoes `cmp r4,#0x35 / beq 0x0800B8B2`, handles the new bytes, and
otherwise jumps to `0x0800B96A`, so the idle reload and the return still run.

**Space.** Free flash runs 0x0801B318–0x0803FEFF (**146 KB**, all of sector 5 except the marker). Free RAM starts
above 0x2000A840. The bootloader copies everything up to the highest packet written, so code placed in sector 5 is
included. Even simpler: append the patch right after the image (0x0801B318).

**A continuous-measure ("stream") mode is plausible in about 100–200 bytes:**

- New command, for example `39` = start, `3A` = stop (both unused in 1.1.4). It sets a RAM flag and presses DIST.
- In the mode-2 path, once a shot is stored (around 0x0800C732, where the mode becomes 2): if the flag is set, go
  straight back to mode 1 with the laser on, `0x200000BA` preset to 40 or more (no second warm-up), and press DIST
  again. Each further shot then takes about the measurement time (~0.4 s, roughly 2.5 shots/s), not 1.5 s.
- Each pass also needs a `0x0800BBA8` call (stay awake), the 1800-tick laser timeout disabled while streaming,
  and a stop on `3A`, on CLR, or after N shots.
- **Throughput limit:** shots go out one at a time. The next is sent only after the phone's ACK (0x080055B8,
  2.5 s resend). The phone must ACK at once, or the stored shots back up in the ring (4096 slots, so nothing is lost,
  only delayed).

**Packet build** (record builder 0x08007B4C; on the wire `[kind][record bytes 1–63]`, sent by 0x080055B8):

| Bytes | Field |
|---|---|
| 0 | kind: 1 shot, 2 calibration (= flag bit 0) |
| 1 | flag: `~cal & ~(leg<<7) & (1 \| mark<<1)` |
| 2–4 | distance (bits 16–23, 0–7, 8–15), mm |
| 5–10 | azimuth, inclination, roll |
| 11–16 | absG, absM, dip (sensor 1) |
| 17–20 | **timestamp, u32 whole seconds** (0x2000003C) |
| 21–44 | raw G/M vectors (4 × 3 × int16) |
| 45–53 | accuracy-error block |
| **54–55** | **unused, always 0xFF** |
| 56–57 | quality (0x20001338+0x10) |
| 58–63 | sensor 2 absG/absM/dip |

**There's room for a counter:** bytes 54–55 are spare in every packet. A u16 shot counter, or a 25 ms tick stamp
for sub-second ordering, fits there with no format change. Readers that ignore those bytes keep working (cavway.rs
already does). For a scan bit, the patch would clear flag bit 6 itself (the 1.1.6 convention).

**Flashing:** the GPLv3 CavwayAssist flasher, or Nathan's web port, sends the patched `.bin` with the same 256-byte
header. Boot mode is entered by hand (arrows + power), never from the app.

## Backing up the firmware

- **Bluetooth can't do it.** `3D` reaches only the windows above (at most 124 bytes at 0x08004000 or 0x08000040). The
  bootloader has write-only commands.
- **Options, in order:** ask Cavway for the 1.1.6 `.bin`; look inside the Cavway phone app's package for a bundled
  image; dump all 512 KB over SWD (PA13/PA14) with an ST-Link. Neither image sets read protection, but the factory
  might have, and the pads aren't located yet.
- **A one-command, read-only check** worth running on the real X1: `3D 00 E0 7C`. It should return the 1.1.6 version
  block, and if it returns more or different data, the read map changed in 1.1.6.

## Read from the real X1 (2026-11-17, `x1-peek`, read only)

`run-mac.sh x1-peek E000:124 E004:4 8008:4 8000:4` on CavwayX1-0312 (`3d` reads only; 0 stored shots were waiting):

| Address | Bytes | Meaning |
|---|---|---|
| E000 | `01 01 07 00`, 28 zero bytes, then `df f8 0c d0 00 f0 d2 fd 00 48 00 47` `c5 58 01 08` `70 a8 00 20` … | **App version 1.1.7.** Same layout as 1.1.4: the version block, then the ARMCC startup at 0x08004020. Entry `__main` is now **0x080158C5** (1.1.4: 0x08013C31) and the stack top is **0x2000A870** (1.1.4: 0x2000A840). The code is about 7.3 KB longer before `__main`. The 1.1.4 addresses in this file do not carry over to 1.1.7. |
| E004 | `0a 00 00 00` | Bootloader version 0x0A, the same as the 2024 full image: **the bootloader has not changed**, so the recovery path above holds. |
| 8008 | `38 01 00 00` | 0x08000044 = **312 = the serial number** (CavwayX1-0312). It sits in the bootloader sector, which updates never erase. In the 2024 full image this word is 0xFFFFFFFF, so it is written per unit at the factory. |
| 8000 | `72 45 fc 6a` | Clock = 0x6AFC4572 = 12:15:46 on 2026-11-17 when read as UTC, while the real UTC time was 17:16. The clock counts **local wall-clock seconds** (the X1 has no time zone). This is the packet's timestamp. |

So 1.1.7 kept the memory-read map (E000, E004, 8000 and 8008 all answer as in 1.1.4) and the dispatcher's `3d` path.
**Backing up the firmware over Bluetooth is still impossible**: E000 is a fixed 124-byte window at 0x08004000.

## USB: a second, wired command port (2026-11-17)

The X1's USB-C port is a **WCH CH343 USB-serial bridge** (VID 0x1A86, PID 0x55D3, "USB Single Serial"). On the Mac it is
`/dev/cu.usbmodem5CBC0132551`, at 115200 baud. It is wired to **USART6**:

- **App:** a second receive path (DMA2 Stream 1, ring at 0x200004D4, line parser 0x08005F24) feeds the same `data:`
  frames to the same dispatcher, with source 0 instead of 1 (`0x08005EE2: bl 0x0800B7FC`). Every command in section 2
  works over USB too. Replies come back unframed (`3d addr n data`).
- **Bootloader:** its byte reader (0x080005E4) polls **USART2 (Bluetooth) and USART6 (USB)** and answers on whichever
  spoke last. So **flashing works over the cable**. That is the COM port CavwayAssist uses, and it is steadier than
  Bluetooth. Boot mode still has to be entered with the buttons.
- **Verified:** `3d` reads over USB return what Bluetooth returned: E000 → `01 01 07 00…`, E004 → `0a`, 8008 → `38 01`
  (scratch script `usbpeek.py`, reads only).

The bootloader byte reader also shows **how to leave boot mode without flashing**: while it waits, a press of **CLR
powers the X1 off** (PC3 low → 0x080009D4 clears the PB10 power latch). If no byte arrives for a long time, it also
powers off (a countdown from 0x51BE5630 loop passes; the time in seconds is unknown).

## Can installed firmware be imaged without hardware? No (checked 2026-11-17)

Every software door was checked in 1.1.4 code (the bootloader is the same in 1.1.7):

- **`3d` read**: fixed windows only (section 2). The shot-ring index is computed from the RAM head counter, which
  could reach any flash address if it were corrupted, but nothing lets us write it:
- **`3e` write** (0x080118F4) only accepts 0x9000–0x90FF (the external EEPROM, below, and coefficients in RAM at
  0x2000101C / 0x200010AC), 0x8000 (set the clock), 0x8008 (write the serial number, only while blank) and 0xF000.
  There is no general RAM write, so no pointer can be redirected.
- **Bootloader**: K/L/M/J/4 only. `L` echoes a CRC of what it just wrote, and only inside staging (0x08040000 up). It
  has no read command. Anything that would run our own dumper must first replace the app, the very thing to save.

**The only direct backup is SWD**: an ST-Link (or J-Link) on PA13 (SWDIO), PA14 (SWCLK), GND and 3.3 V, reading
0x08000000–0x0807FFFF. **Danger:** if the factory set read protection (RDP level 1), the read returns nothing, and
the only way to unlock is a mass erase, **which destroys the firmware**. Never accept a tool's offer to "unprotect".

Side find: `3d`/`3e` 0x9000–0x90FF talk to an **external I2C EEPROM at address 0x50** (bit-banged, 0x0800B506 /
0x08011746). That is very likely where calibration lives, which would mean **calibration survives a firmware update**.
Not yet confirmed.

**USB answers only while charging (found 2026-11-17, 1.1.7).** The app's USB sender (0x08006418) sends only when
0x20000024 is set, and 0x0800BEA8 sets that flag from PB2 low, the charger's "charging" pin (the same flag puts
"Charging" on the Information page). With a full battery, commands over USB still arrive and run, but nothing comes
back. Bluetooth is unaffected. Candidate for cc3: send whenever USB power is present.

**The clock** (memory 0x8000, u32 local seconds) can be set with `3e 00 80 04` + 4 bytes on every firmware seen.
X1 Flasher sets it after every flash, `x1flash clock` sets it by hand, and CaveCAD sets it on every connect.

## Public releases, and scan mode in 1.1.7 (2026-11-17)

**Cavway publishes every release** on <https://cavedevice.com/blogs/cave-survey/cavway-firmware-downloads> (Google
Drive links). Downloaded to `~/Documents/github/cavway-firmware/` (kept out of git):

| File | sha256 (first 16) | Built (header) | Payload |
|---|---|---|---|
| `CVWY_X1_1.1.7.bin` | `18060d77000d45dd` | 2026-11-06 11:41 UTC | 102712 B, 803 packets, ends 0x0801D138 (139 KB free) |
| `CVWY_X1_1.1.6.bin` | `f7a3020403b72928` | 2026-03-22 06:21 UTC | 98856 B, 773 packets |

Release notes: 1.1.6 added scan mode and a force-pair switch. 1.1.7 added the shot menu and memory clear, better
calibration grouping, **no power-off during data transmission**, and a calibration overflow fix. 1.1.4 already said
**"firmware upgrades no longer erase calibration"**, which fits the external EEPROM.

**The X1's own 1.1.7 is a different build from the public file.** On the device, `__main` = 0x080158C5 and the stack is
at 0x2000A870. In the public 1.1.7 they are 0x08015925 and 0x2000A880. The version block is the same `01 01 07 00`.
Flashing the public 1.1.7 is the nearest restore available, but it is not byte-for-byte what is on the unit now.

**Provenance (Nathan, 2026-11-17):** the unit was handed over by Cavway's developer and was used heavily on the Philippines expedition (October 2026). Its build sits between 1.1.6 and the public 1.1.7 in size (`__main` 0x08014A7D < 0x080158C5 < 0x08015925; stack 0x2000A860 < 0x2000A870 < 0x2000A880). So it is most likely a **pre-release 1.1.7**, field-proven but **not recoverable** once overwritten. Before flashing: ask the developer for that exact `.bin`, The shots now in its memory are bench tests only; Nathan says they can be discarded (2026-11-17).

All addresses below are in the **public 1.1.7**.

### Keys and modes

Key structs: DIST 0x20001290 (PA8), M 0x2000129A (PC0), `<` 0x200012A4 (PC1), `>` 0x200012AE (PC2), CLR 0x200012B8 (PC3).
The mode byte is now **0x20000068**. The UI state machine is 0x0800CFE0, with a jump table at 0x0800CFF2. Mode 1 =
laser on (0x0800D100), **mode 2 = scan (0x0800D440)**. Scan options are one settings byte, **0x20001044 + 0x16**:

| Bits | Meaning (as the code uses them) |
|---|---|
| 0 | Scan allowed |
| 1–2 | Steadiness level, index into int16 table 0x08017BBE = 500, 2000, 10000, 1 |
| 3 | Which key starts it: 0 = DIST held + M, 1 = long `>` |
| 4–7 | Time limit index, into 0x08017BC4 = 1, 2, 5, 10 (minutes) |

### Entry (0x0800D1EC, in mode 1 with the laser on)

DIST long press (event 2) with **M physically down** (PC0 low) and `<` up, if option bit 3 = 0. Or a long `>` if bit 3
= 1. Then, only if bit 0 is set: mode = 2, shot counter 0x20000070 = 0, tick counter 0x200000C0 = 0, mark = none, beep,
`bl 0x08016098` (continuous laser).

### The scan loop (mode 2, every 25 ms tick)

1. `0x080061B8`: a fresh laser reading is ready (flag 0x200012F8).
2. `0x080097FC(level)`: the instrument is steady (four motion values below the chosen level).
3. More than 10 ticks (0.25 s) since scan started.
4. `0x0800A0D8(…, 50)`: the direction differs from the last stored shot by more than 50 units (0.27°), so it only
   stores when it has moved.

When all four hold: counter + 1, remember this direction, store the shot with `0x08013294(0, 0, 3 if first else 2,
mark)`, send it (`0x0800C6F0`), beep. The record flag is `~(leg<<7) & ~(type<<5) & ~cal`, so type 2 clears bit 6
(**scan**) and type 3 clears bits 5 and 6 (**first of a sweep**). That matches what cavway.rs decodes.

**Pacing is by motion, not by time**: one shot per fresh reading, whenever the X1 is steady and pointing somewhere new.

### Exit (0x0800D4D2)

- a **CLR** press, or
- ticks > limit × 2400 (**1, 2, 5 or 10 minutes**), or
- **more than 200 shots** (counter > 0xC8).

Then: laser off (`0x08011B60`), beep, mode 1.

### Unchanged from 1.1.4

- **Dispatcher `0x0800C32C`**: same codes. `36` is now allowed in modes 0, 3 and 0x31, `38`/`35` in 1 and 0x31, `37` in 1.
  **No remote scan start.** The same 4-byte hook point is at **0x0800C3DE** (`cmp r4,#0x35 / bne 0x0800C49A`).
- **Packet bytes 54–55 are still unused**: the builder (0x0800802C) writes the accuracy block at 45–53, then 56 on.

### Patch plan on the public 1.1.7 (built as `1.1.7-cc1`: see tools/x1-patch/README.md, not yet flashed)

Everything goes in free flash after 0x0801D138, with the dispatcher hook at 0x0800C3DE:

| New byte | Does | How |
|---|---|---|
| `39` start scan | Laser on if needed, then enter mode 2 | Copy the entry block 0x0800D230–0x0800D250 (mode = 2, clear counters, mark = none, beep, `bl 0x08016098`), skipping the key check. Optionally force option bit 0. |
| `3A` stop scan | End scan cleanly | Set the CLR event byte (0x200012B9 = 1) while mode == 2; the existing exit path does the rest. |
| `3B` status | Mode, scan count, ticks, options | One reply with mode (0x20000068), counter (0x20000070), ticks (0x200000C0), options byte. |

Signalling: write the scan counter (0x20000070, u16) into **packet bytes 54–55** in the builder, and send short
`3C`-style status frames on scan start and end (reason: CLR, time, 200, remote). Make the 200 cap and the time limit
patchable constants. Keep-awake: `33` still reloads idle; 1.1.7 already stops power-off during transmission.

**Restore path:** flash the public 1.1.7 (or 1.1.6) with X1 Flasher. The bootloader is untouched, so this always works.

## Still unknown

1. ~~Scan mode~~ mapped in the public 1.1.7 (above). Still open: what `0x0800A0D8` and `0x080097FC` measure exactly, and the steadiness units.
2. Whether 1.1.7 kept the "any frame keeps it awake" rule and the new byte codes (the read map is confirmed kept).
3. Which arrow is PC1 and which is PC2, and whether DIST really is the power key (bench check: arrows + DIST →
   "BOOT MODE", then power off with CLR long press or `34`).
4. Calibration persists across updates (Cavway's 1.1.4 notes), very likely in the I2C EEPROM.
5. The exact MCU part and its RAM size; whether option-byte read protection is set; where the SWD pads are.
6. What the 0x9000 memory block and the 0x0803FF00 one-time init contain.
