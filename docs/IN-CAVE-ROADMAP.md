# In-cave survey with the Cavway X1: roadmap (2026-11-16)

Where in-cave surveying with the X1 stands, and what comes next. Builds on `CAVWAY-LINK-AND-SWEEP-PLAN.md` (the link,
the scan flag, sweeps as rings) and `CAVWAY-X1.md` (the wire format).

## Built (branch `ble-bridge`, test builds only)

- **In-Cave Survey layout** (Survey tab and View tab → Layout): the live Map on the left and the **Device Capture**
  panel on the right, full height; under the map, the Cross Section with Profile, Notebook and Stations as tabs.
  Map locked. Sized for an iPad held landscape (checked at iPad mini 1133×744 and iPad Air 1180×820).
- **Device control** at the top of Device Capture: Laser on / off (X1), and the two sweeps.
- **Sweeps with live checks** (`crates/cave-sweep/src/check.rs`, tested):
  - **LRUD ring**: across the passage at the standing station. Checks: enough shots across the passage, every wall
    reached (ceiling, floor, left, right), no big gap round the ring; shots pointing along the passage are noted and
    not counted. Shows the L/R/U/D the ring gives.
  - **Room sweep**: laser about level, turned right round. Checks: enough level shots, no compass direction left
    out (it names the direction: "Nothing towards the south-east yet"); steep shots are noted, not counted.
  - A coverage **dial** fills in as shots land; the card turns from amber to done.
  - **Digital with an X1 connected: one button.** LRUD ring or Room sweep readies CaveCAD and asks the surveyor to
    hold DIST+M (no command to start the X1's scan from outside is known yet). The X1's scan shots are followed
    live, and the surveyor ends the scan on the X1 itself: CaveCAD finishes the sweep when the X1's next ordinary
    shot arrives, when the scan stream goes quiet for 8 s (the X1 sends about 1.5 shots a second while scanning), or
    at the X1's 200-shot limit.
  - With other instruments (or none), started from the screen, every shot until **Finish sweep** is a sweep shot (a splay, never part of a leg, tagged
    as one sweep). With "CaveCAD fires the laser" on, CaveCAD fires the X1 every 1.5 s and stops by itself once the
    check passes (or at 200 shots, the X1's own limit). Or the surveyor holds DIST+M for the X1's own scan mode:
    CaveCAD recognises the scan flag and checks it the same way (as an LRUD ring or a room sweep, chosen in the
    panel).
  - Starting a sweep brings the Map to the station and cuts the Cross Section there, so the ring is seen filling in.
- **Manual / Digital standard** (switch in Device control, kept per device):
  - **Manual**: the survey is mostly sketched by hand. LRUD is four aimed shots called in LRUD order (left, right,
    up, down): CaveCAD names the next wall, and each shot is written as that wall's dimension at the station, not as
    a splay. A room outline needs a rough eight-way sweep (8 level shots, no gap over 90°). CaveCAD never fires on
    its own in Manual.
  - **Digital**: the Cavway captures the detail. A dense ring at every station (12+ shots, 2 per wall, no gap over
    60°), rooms all the way round (24+ level shots, no gap over 45°). The panel reminds the surveyor to sweep each
    new station before moving on.
- Simulator: sweeps work against the simulator (Survey tab → Simulator), so all of this can be tried with no X1.

To check on the real X1 next time it is in hand:

- [ ] Remote trigger `0x38` fires a shot (CaveCAD's own firing depends on it). Laser on `0x36` / off `0x37`.
- [ ] A sweep fired by CaveCAD at 1.5 s: does the X1 keep up, and does each shot come back before the next fire?
- [ ] **Remote scan start** (for true one-button capture): no command is documented. Ways to find it: ask Cavway;
      capture the bytes their own app sends if it has a scan button; read the status word at `0xC044` before, during
      and after a scan (it may also say when a scan ends, which would beat the 8 s quiet rule); as a last resort,
      try the unused single-byte commands `0x33`, `0x35`, `0x39`… one at a time (never `0x34`: power off).
- [ ] The scan's end: does anything arrive when CLR ends a scan or the time limit hits? (Capture it.) Is 8 s of
      quiet right at the Low speed setting?
- [ ] Old memory on connect still lands as splays at the current station (from first contact): must be offered,
      not written. Fix before field use.

## iPad: direct device support (iPad, iPad Air, iPad mini)

Target screens, landscape, in points: iPad mini 1133×744, iPad 1180×820, iPad Air 11" 1180×820, iPad Air 13"
1366×1024. The In-Cave Survey layout is checked at the smallest (mini).

The catch: **Safari on iPad has no Web Bluetooth**, so cavecad.com in Safari cannot talk to the X1. Two ways round
it, in order:

1. **Now, no install from us: the Bluefy browser** (free, App Store) at cavecad.com. It has Web Bluetooth. To check
   early: write without response, notification rate during a fast sweep, reconnect after the X1 sleeps.
2. **The native CaveCAD app for iPad** (Tauri 2's iOS build of the same app). The Bluetooth bridge already in the
   desktop app (`btleplug`) also runs on iOS through Apple's CoreBluetooth, so the instrument code is shared. Also
   gives offline use with no browser in the way.

### Native iOS without paying the $99 up front

- **Local testing is free.** Xcode signs with a free Apple ID ("Personal Team") and installs straight onto your own
  iPad over a cable. Limits: the app stops opening after **7 days** and must be installed again from Xcode (one
  click, the cave data stays); at most 3 such apps on a device; no TestFlight, no sharing with other cavers.
  Bluetooth works this way. The iPad Simulator has no Bluetooth, so the X1 needs the real iPad.
- What it takes: Xcode is on this Mac (27.2). One-time on the iPad: Settings → Privacy & Security → Developer Mode
  on, and trust the developer under Settings → General → VPN & Device Management after the first install.
  In the app: `tauri ios init`, the desktop-only plugins (updater, single instance) kept out of the iOS build, the
  Bluetooth usage text in Info.plist, and the free team picked in Xcode's Signing settings.
- **Paid only when sharing**: TestFlight (other cavers testing it) and the App Store need the Apple Developer Program.
- **Non-profit route**: Apple waives the $99 a year for recognised non-profit organisations (in the US, a 501(c)(3))
  that publish free apps. It needs the organisation as a legal entity with a D-U-N-S number, enrolled as an
  organisation (not as a person), and Apple's review takes a few weeks. Worth it if a grotto, the NSS or a CaveCAD
  non-profit would own the app. It changes nothing for local testing, which is free anyway.

## Gimbal automation (planning only, nothing built)

Gimbal in hand: **DJI Osmo Mobile** (a phone gimbal). Idea: the X1 sits in the phone clamp, CaveCAD turns the
gimbal step by step and fires the X1 at each stop, so a sweep is even and complete with no hand wobble.

- **How a sweep would run**: the surveyor sets the gimbal on the station (a small tripod over the station, height
  noted), picks LRUD ring or Room sweep, and presses Start. CaveCAD turns the gimbal to each aim point (e.g. every
  15° round the passage axis for a ring, every 10° in yaw at level for a room), waits for it to settle, fires the
  X1 (`0x38`), waits for the shot, then moves on. The live check is the same one built now; the gimbal just fills
  the gaps it names. Aim points come from the check's empty sectors, so a re-sweep only covers what is missing.
- **The open question: can the Osmo be driven from outside?** DJI's old Osmo Mobile SDK covered only the first
  models and is retired; current Osmo Mobile models are driven by the DJI Mimo app only, with no public SDK. The
  community has worked out parts of the Bluetooth protocol for some models (unofficial, to be verified on the exact
  model in hand). Steps: find the model number; capture its Bluetooth traffic while Mimo moves it (same method as
  the X1); see if absolute pan/tilt commands exist. If they do not, a fall-back is an open pan-tilt head (two small
  motors and a Bluetooth board) that CaveCAD can drive with documented commands.
- **Things the hardware must allow**: the X1's weight and balance in the clamp; the gimbal's tilt range (an LRUD
  ring needs straight up and straight down, which phone gimbals often cannot reach: a ring may need two passes, or
  the X1 turned 90° in the clamp); the compass reading next to the gimbal's motors (magnets near the X1 can pull the
  compass: test with a known bearing before trusting it, and calibrate the X1 in the clamp if needed).
- **Where it fits**: after the X1 checks above pass and the iPad app runs, as its own project.
