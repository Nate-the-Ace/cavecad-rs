# Cavway X1: direct link and splay sweep, the plan (2026-11-13)

The X1 is in hand. This plan covers two things: getting the shots straight into CaveCAD, first on the laptop and
then on the iPad in the cave, and the X1's **scan mode** (the splay sweep). It builds on `docs/CAVWAY-X1.md` (the
wire format, worked out without the instrument) and `docs/INSTRUMENTS.md`.

## Where things stand

Already built, never run against a real X1:

- Rust (`crates/cave-instrument`): the decoder, framing, acknowledgement, leg finding (three close shots = one leg),
  marks, accuracy errors, memory queries, a simulator. `crates/cave-session` turns shots into notebook edits.
- Browser (`apps/web/src/lib/instrument/webble.ts`): Web Bluetooth connect, the capture log, the Survey panel.
- Hidden behind a feature flag. **Test builds now show it** (`features.ts`: `VITE_UPDATE_CHANNEL === 'dev'`).

## The one technical catch: where Bluetooth works

| Where | Web Bluetooth? | So |
|---|---|---|
| Chrome or Edge on the Mac, Windows, Linux | yes | works today at cavecad.com, the first test bench |
| CaveCAD desktop app on the Mac (Safari's engine) | **no** | needs a native Bluetooth bridge |
| CaveCAD desktop app on Windows (Edge WebView2) | not enabled | same bridge |
| iPad Safari | **no** | native app, or the Bluefy browser as a stopgap |
| iPad, Bluefy browser | yes | in-cave stopgap, no install from us |

Decision: one **native Bluetooth bridge** in the Tauri shell (`btleplug`, which also covers iOS through
CoreBluetooth) that hands raw notifications to the same Rust link the browser path uses. Both paths share every
byte of protocol logic, so a fix in one fixes both. The page picks whichever transport is there: Web Bluetooth in a
browser, the bridge in the app.

USB (Web Serial) stays for history download and calibration on a computer (the Cavway Assistant port does that).

## Phase 1: first contact (today, on the laptop)

1. Use Chrome at cavecad.com with `cavecad.instruments` on, or a test build (Account → Updates → Test builds).
2. Connect, turn the capture log on, and work through the checklist below. The log keeps every raw packet, so
   anything that surprises us can be replayed in tests afterwards. Save the captures into
   `crates/cave-instrument/tests/captures/` as the real-device test set.

Checklist (the open questions from `CAVWAY-X1.md`, plus the sweep):

- [ ] Advertised name (for the chooser filter).
- [ ] Packet size over BLE: 33 or 64 bytes.
- [ ] Shots push on their own after connect? Shots taken while connected arrive at once?
- [ ] Ack `flag | 0x55` stops the re-send.
- [ ] Leg flag: bit 7 (Windows app) or bit 1 (PDF)? On all three close shots or only the third? A fourth?
- [ ] Backsight mark: azimuth about 180 degrees off, clino flipped.
- [ ] Remote trigger `0x38` fires a shot; laser on/off `0x36` / `0x37`.
- [ ] Shot past 65.5 m (if there's a long enough room).
- [ ] **Scan shots: which flag bit says "scan".** The PDF calls bits 2 to 7 "not used" and the Windows app reads
      only bit 7 (leg), bit 0 (calibration) and bits 1 to 3 (mark). No public source decodes the `S` the X1 shows
      in memory. Take a sweep, compare the flag bytes with ordinary splays. Also: do scan shots stream live, one by
      one, during the sweep, or arrive after it ends?
- [ ] Scan speed Low / Medium / High: shots per second and how far apart they land.
- [ ] Reconnect after the X1 sleeps or walks out of range: are unsent shots re-sent, nothing doubled?

## Phase 2: the bridge and the test builds

- Tauri commands `ble_scan`, `ble_connect`, `ble_write`, plus a notification event, on `btleplug`. The web side gets
  a second transport beside `webble.ts` with the same shape.
- macOS needs the Bluetooth usage string in `Info.plist`.
- Ship it as a test build: `gh workflow run desktop.yml --ref <branch> -f channel=dev`, then
  `deploy/update-downloads.sh --dev`. Admins switch Account → Updates to Test builds once, then it self-updates.

## Phase 3: the splay sweep

### What the X1 does (manual, Rev. C)

Hold DIST+M (or `>`, set in the menu) for 2 s: "SCAN MODE". The laser stays on. Each time the instrument is held
still for a moment it takes a shot on its own. It stops on CLR, at the time limit (1, 2, 5 or 10 min), or after
**200 shots**. Speed Low / Medium / High trades accuracy for pace. Scan shots show `S` in memory.

So a sweep is: stand at a station, sweep the laser round the passage, and get up to 200 splays in a minute or two.
That captures the passage shape at that station far better than four LRUD shots.

### What CaveCAD does with it

1. **Know a sweep when it comes.** Scan shots must never be taken for a leg: while sweeping, the laser rests on
   nearby wall points, and three of those in a row can pass the "close shots" test. Scan-flagged shots skip leg
   finding entirely. Until the bit is confirmed (Phase 1), a **Sweep** button in the Survey panel says "the shots
   coming now are a sweep" by hand. The button stays useful afterwards for instruments with no scan mode.
2. **Keep it as one thing.** A sweep is a group of splays at one station: its shots in order, time, speed setting,
   and which ones were thrown out. Notebook: one row, "Sweep at A12: 143 shots", which opens out. Every shot is
   still a plain splay, so exports and older tools see ordinary splays.
3. **Clean it.** Drop shots with accuracy errors or no return, isolated jumps (a shot that lands on a caver or
   beyond a gap), and near-duplicates. Thrown-out shots are kept, greyed out, and can be put back.
4. **Turn it into drawing.**
   - **Cross-section:** fit the plane the sweep mostly lies in (usually across the passage), order the points
     round the station, and draw the outline. This drops straight into the cross-section sketching (station-pinned
     sections already exist).
   - **LRUD, derived:** left, right, up, down taken from the outline, so Compass export and the plain passage
     drawing get real numbers with no extra shots.
   - **Plan and profile walls:** the sweep's points show as wall hints under the sketch to trace over.
   - **3D:** each sweep becomes a ring; the 3D view joins ring to ring instead of LRUD boxes.
5. **Live on screen while sweeping** (this is the iPad's job in the cave): the shot count and the outline filling
   in, so the surveyor can see a gap ("nothing on the ceiling yet") and sweep there before moving on.

### Exports

- Survex, Therion: every kept shot as a splay (Survex `*flags splay`, Therion splay legs), plus the LRUD.
- Compass: LRUD only (it has no splays).
- CaveCAD's own file: the whole sweep, thrown-out shots included.

### Built in this order

1. Simulator: an X1 "sweep" quirk (a ring of shots round a known passage shape, at the three speeds, with a few
   bad shots). Lets everything below be built and tested before the bit is known.
2. Recognise and group (flag bit once known, Sweep button meanwhile); no false legs. Tested against the Phase 1
   captures.
3. Notebook row and the clean-up.
4. Cross-section outline and derived LRUD.
5. Live sweep view.
6. Wall hints in plan and profile; 3D rings.

## Phase 4: the iPad in the cave

- Stopgap first: Bluefy on the iPad at cavecad.com (Web Bluetooth) works with no app from us. Check it early: the
  questions are writes with or without response, notification rate during a fast sweep, and reconnecting.
- Proper: a Tauri iOS build of the same app with the native bridge, through TestFlight (Apple's beta channel, so
  admins only, like test builds). Needs an Apple developer account ($99 a year) and offline sign-in, since there's
  no signal in a cave. The offline pass already covers this.

## Questions for Nathan (product, not code)

1. How will you sweep: one slow ring across the passage at each station, or spray the whole room? The first gives
   clean cross-sections. The second gives better walls and 3D but messier sections.
2. What do you want first from a sweep: the cross-section drawing, the plan walls, or the 3D?
3. Should a sweep fill in LRUD automatically and quietly, or ask first?
