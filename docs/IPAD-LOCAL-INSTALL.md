# CaveCAD on your own iPad, free (no Apple Developer Program)

The native iPad app is the same CaveCAD as the desktop app (Tauri's iOS build), with the Bluetooth bridge, so the
Cavway X1 connects directly. For testing on your own iPad it is signed with a free Apple ID ("Personal Team").

Limits of free signing: the app stops opening after **7 days** (press Run in Xcode again; the cave data stays), at
most 3 such apps on a device, and no TestFlight (it can't be sent to other cavers). Paying the $99 a year, or the
non-profit fee waiver (see `IN-CAVE-ROADMAP.md`), removes those limits.

## One time, on the Mac

1. **Xcode → Settings → Accounts → +** and sign in with your Apple ID. A team called "<your name> (Personal Team)"
   appears.
2. Tools (already installed on Nathan's Mac 2026-11-16): Xcode, `rustup target add aarch64-apple-ios
   aarch64-apple-ios-sim`, `brew install cocoapods` (Tauri also installs `xcodegen` and `libimobiledevice`).
3. The Xcode project is kept in git (`apps/desktop/src-tauri/gen/apple`), with one hand edit: CoreBluetooth in
   `project.yml`. After changing `project.yml`, run `xcodegen generate` in that folder.

## One time, on the iPad

1. Plug it into the Mac with a cable; on the iPad, tap **Trust This Computer**.
2. **Settings → Privacy & Security → Developer Mode** on (the iPad restarts). The switch shows up after Xcode has
   seen the iPad once (step 2 below).

## Install (and every 7 days)

1. `cd apps/desktop && npx tauri ios open` opens the project in Xcode.
2. Top bar: pick your iPad as the run destination.
3. Target **cavecad-desktop_iOS → Signing & Capabilities**: tick *Automatically manage signing*, Team = your
   Personal Team. If Xcode says the bundle identifier is taken, change it to something only yours, such as
   `app.cavecad.nathan`.
4. Press **Run** (▶). The first time, the iPad refuses to open it: **Settings → General → VPN & Device Management →
   your Apple ID → Trust**. Run again.
5. When CaveCAD asks to use Bluetooth, allow it; then Survey tab → Connect finds the X1.

## Checked so far

- The iPad build compiles and runs in the iPad Simulator (iPad Air 11", 2026-11-16): the app starts, Sign in opens
  Safari at cavecad.com, and a `cavecad://` link comes back into the app and is handled. The Bluetooth permission
  text is in the app. (No Bluetooth in the simulator, so the X1 needs the real iPad.)
- Polish owed: the white strips above and below the app (the page should paint under the status bar and home bar).
- Not yet on a real iPad: needs your Apple ID in Xcode (step 1 above) and the iPad on a cable.
- Desktop-only parts are left out of the iPad build: self-update (Xcode installs new versions) and the
  one-copy-at-a-time rule. Picking a folder for a cave may behave differently on the iPad: to check.
