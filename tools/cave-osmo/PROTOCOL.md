# DJI Osmo Mobile 7 over Bluetooth LE: the protocol

What it takes to drive a DJI Osmo Mobile 7 phone gimbal from your own software, worked out from captures of DJI Mimo
and tested on the gimbal (firmware as shipped, late 2026). DJI publishes none of this. The day-by-day log, with every
measurement behind these notes, is `FINDINGS.md`; working code is `src/lib.rs` (Rust, btleplug) and the CaveCAD app's
`apps/web/src/lib/instrument/osmo.ts` (TypeScript, Web Bluetooth).

Only the OM7 was tested. Older Osmo Mobiles speak the same DUML framing; whether they take the same commands is
unknown.

## Link

| | |
|---|---|
| Service | `0000fff0-0000-1000-8000-00805f9b34fb` |
| Write | `fff5`, write without response, 20-byte chunks |
| Notify | `fff4` |
| Advertised name | `OM7-…` |

**Bonding trap.** macOS and iOS bond the OM7 as a HID device on first connect, reconnect it at once, and the gimbal
then stops advertising: a scan never finds it again. Ask the OS for already-connected peripherals with service `fff0`
(`retrieveConnectedPeripherals(withServices:)` on Apple platforms) as well as scanning.

**One central at a time.** If a phone (or the Mac) holds the gimbal, nobody else can connect.

## Framing (DUML)

```
55 | len lo | len hi (bits 0–9 length of the whole message, bit 10 set) | crc8 |
sender | receiver | seq lo | seq hi | attributes | command set | command | payload… | crc16 lo | crc16 hi
```

- **CRC-8**: reflected polynomial 0x31 (table poly 0x8c), init 0x77, over the first 3 bytes.
- **CRC-16**: reflected polynomial 0x1021 (table poly 0x8408), init **0x3692**, over **the whole message** before it,
  header and CRC-8 included. (Community projects seed 0xDF0C over the body only; that matches only for 21-byte messages,
  which is why every other command they send is silently dropped.)
- Addresses: `02` app, `04` gimbal, `27` handle board.
- Attributes: `00` push, `40` request wanting an answer, `80` set on answers, `20` "answer me" (handshake).

## Handshake

After connecting the gimbal sends `00/00` with attributes `0x20`. Answer it — `00/00`, attributes `0xc0`, payload
`00 <its first payload byte>`, **its sequence number**, addressed back to the sender — and it starts reporting.

## What it reports

- **Attitude, `04/05`, once a second** (no faster; polling gets no answer). Payload bytes 24–39: a unit quaternion
  `w x y z` (little-endian float32), the clamp's attitude in north-east-down, north = wherever the gimbal woke up (no
  compass). Int16 tenths of a degree at 4, 20, 22: pan, tilt and roll **joints**. In portrait the clamp's long axis is
  body +Y.
- **Handle thumb stick, `04/57` from `27`**, ~18 a second: x, y int16, ±660. With an app connected the stick is
  forwarded to the app instead of moving the gimbal.
- **Target-mode status, `04/6b`** (below).

## Moving it

### Speed (a joystick)

`04/0c`, payload `yaw roll pitch` (int16, tenths of a degree per second), `0x80`, `0x02`. Keep sending every ~100 ms;
it stops ~0.2 s after the last one. Timed turns land within a few per cent (about 2° short per move).
**Speed commands can flip the clamp**: past a roll joint of ~63 or ~221 (portrait) it swings over violently, at any
speed.

### Target mode (DJI's own positioning — use this)

How Mimo runs a panorama. The gimbal is given a list of **attitude targets relative to home** (not joint angles: it
works out its motors itself) and steps through them. It never flips, takes each move in the time it is given, and says
when it has arrived. Accuracy on the bench: every stop within ~0.2–3°.

1. **Enter**: `04/4c [01 00]`, `04/0f [22 01 00]`, then `04/6b [00 00 00 6d]` (all attributes `40`). **Entering
   sends the gimbal home.** Give it ~2.5 s before the next step.
2. **Upload a list** (`04/6d`, attributes `40`):
   ```
   01, L, 2c 00 00 0a, N, N × (u16 duration, i16 yaw, i16 roll, i16 pitch), id
   ```
   L = payload length − 2; angles in tenths of a degree; **duration in hundredths of a second** — the time the move to
   that target takes (Mimo sends 60 = 0.6 s; 300 makes a 45° move take ~3.5 s, 1000 about 10 s); `id` any byte, echoed
   in the status. The gimbal sets off for target 0 at once. Answer `00 01 01 00` = accepted; a last byte of `10`, `80`,
   `90` flags a problem (`80`: a target was clamped; `90`/`10`: refused).
3. **Go to target i**: `04/6d [00 06 40 00 i 0a 00 00]`. Answer `00 01 01 00` accepted, `00 00 01 00` refused.
4. **Status push** `04/6b`, attributes `00`: `01 6d 06 01 S 00 i N id` — S `0x40` moving, `0x60` arrived (`0x6c`:
   arrived with the target clamped), `0x20` the list was dropped and the gimbal went home (also sent once, harmlessly,
   just after entering), `0x00` cleared / idle.
5. **Leave**: `04/6b [00 01 00 6d]` — holds where it is; wait for a status with S `0x00` (a leave sent while the gimbal
   is still moving is ignored: send it again). **Exit** like Mimo by following it with `04/6d [01 06 c0 00 00 00 00
   00]` (the gimbal then goes home).

Limits found:

| | |
|---|---|
| Targets per list | **15** (16 or more are accepted but never stepped through) |
| A new list | only after leaving and entering again (a list sent while one is loaded is refused) — and entering sends the gimbal home, so **end each list at home** |
| Idle | target mode ends by itself ~9 s after the last command; to hold a target longer, send "go" to it again |
| Yaw | about −200…+95 (lopsided, like the pan stops); +120 is clamped, +176…180 refused; use the same heading 360° lower |
| Roll | + side to at least +110 (laser 20° past level in portrait); − side capped by the gimbal at ~−77, and with a heavy load on the clamp a quick move there overloads the motors and the gimbal drops the list |
| Pitch | home leans back by whatever the load sags it (~16° with a 150 g instrument); model it rather than command it |

Direction model (portrait, laser along body +Y, home sag `s`): the clamp top points along
`Ry(s) · Rz(yaw) · (0, cos ra, sin ra)`, `ra = −90° − roll` — the sag tilts the whole yaw axis. Every bench point fits
within 2°; straight up is reachable (roll leans the sag back out).

### What does nothing

`04/14` (absolute/relative angle) gets no answer; `04/0a` answers "ok" and does not move; `04/54` (sent by Mimo,
looked like a recentre) is acknowledged and does nothing. A double press of the trigger recentres; nothing found does
that over Bluetooth except going home in target mode.

## Other hardware notes

- The clamp holds a strong magnet: an instrument's compass in the clamp is useless. Distances and inclinations are
  fine; take azimuth from the gimbal's own attitude (relative to where it woke up).
- With a load in the clamp the gimbal sags at home and short-side moves can overload it (above).
