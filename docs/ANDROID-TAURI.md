# CaveCAD on Android (Tauri)

The Android app is the same Tauri app as the desktop, iPhone and iPad apps (`apps/desktop`), built with
`tauri android`: the whole interface is the web app (`apps/web`) in the system WebView, so a phone gets exactly the
iPhone's phone layout (it switches on at a width of 600 CSS px or less). The Android Studio project is kept in git at
`apps/desktop/src-tauri/gen/android` because it carries hand edits (below). The earlier Kotlin prototype
(`app.cavecad.android`) is obsolete; this app is `app.cavecad.desktop`, the same identifier as on iOS.

## Build and install

```sh
tools/android-install.sh [adb-serial]   # default: the first device in `adb devices`
```

It sources `~/android-env.sh`, reads the account settings from the repository variables with `gh` (the same
`VITE_*` settings as `tools/ipad-install.sh` and the desktop test builds, channel `dev`), runs
`npx tauri android build --debug --target aarch64 --apk` (which builds the web app first), installs the APK with
`adb install -r` (caves in the WebView's storage stay) and launches it. A rebuild takes about a minute; the first one
longer (Gradle and the Rust crates).

Launcher icons: `tools/android-icons.sh` makes them from `icons/source.png` (the same square as `tools/ios-icons.sh`:
an adaptive icon, paper colour behind grid and logo, plus legacy and round icons). Run it again after changing the logo.

### Prerequisites (this Mac)

- `~/android-env.sh`: JDK 17, `ANDROID_HOME=~/Library/Android/sdk`, `NDK_HOME`/`ANDROID_NDK_HOME`.
- Android NDK **27.2.12479018** (r27c, 2.4 GB): `sdkmanager "ndk;27.2.12479018"`.
- Rust targets: `rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android`
  (only aarch64 is built by the script).
- `cd apps/desktop && npm install` (Tauri CLI 2.12).
- The generated project uses AGP 9.3.1, Gradle 9.6.1 (wrapper), Kotlin 2.2.10; it downloads SDK platforms 36 and 37
  and build-tools 36 itself on the first build.

### Project settings

- `minSdk 29` (Android 10), `targetSdk 34`, `compileSdk 37` (as generated: Tauri's own Android libraries need 35 or
  newer to compile against; the target stays 34).
- `local.properties`, `build/`, `tauri.properties`, `tauri.settings.gradle`, the generated Kotlin (`.../generated`)
  and the Rust `.so` symlinks are git-ignored by the project's own `.gitignore` files.

## Android specifics (hand edits in gen/android)

- **Orientation:** phones portrait only, tablets turn freely (as iPhone and iPad). `res/values/bools.xml`
  `is_tablet=false`, `res/values-sw600dp/bools.xml` `true`; `MainActivity.onCreate` sets `requestedOrientation`
  from it. `android:configChanges` includes orientation and screen size, so turning a tablet never reloads the page.
- **Edge to edge:** the page draws under the status and navigation bars; `viewport-fit=cover` plus
  `env(safe-area-inset-*)` keep the tab row clear (the WebView reports `safe-area-inset-top` 24 px on the test phone).
  The bar icons are light (the app's default theme is dark). Known gap: with the light theme the status bar icons are
  light on a light page (the page does not tell Android its theme yet).
- **No white flash:** the window background is the page's dark colour (`page_background`, `#151F28`).
- **Keyboard:** `android:windowSoftInputMode="adjustResize"`. Not checked with a real text field yet (the test phone
  could not get past sign-in).
- **Back button:** the page has no history (tabs and sheets are not pages), so Tauri's default (WebView back, else
  close) would close the app from any screen. `MainActivity.onBackPressed` sends the app to the background instead,
  like Home; coming back finds the page as it was (checked: page state survives Back). Back does not close a sheet or
  dialog; that would need the page to listen for Back (Tauri's `back-button` event) and is not done.
- **Deep link:** `cavecad://` (the deep-link plugin writes the intent filter into the manifest from
  `tauri.conf.json` `plugins.deep-link.mobile`). Sign in opens the system browser (Chrome) on the account server, which
  sends the browser back to `cavecad://auth?...`; `lib.rs` hands the link to `window.__cavecadLink`.
- **Bluetooth:** below.

## Bluetooth

`ble.rs` (btleplug 0.11) works on Android through btleplug's Android backend:

- Its Java half and jni-utils' Java half are vendored in `gen/android/app/src/vendor/java` (unchanged copies, versions
  and refresh steps in `src/vendor/README.md`; `app/build.gradle.kts` adds the folder; `proguard-rules.pro` keeps the
  classes for release builds).
- `MainActivity.onCreate` calls the native `initBluetooth()` (`src/ble_android.rs`), which runs `jni_utils::init` and
  `btleplug::platform::init` there, because JNI only finds the app's own classes from a call that starts in Java.
  Logcat says `CaveCAD: Bluetooth bridge ready` (or what failed).
- btleplug calls Java whenever its futures are polled, so on Android every Bluetooth command and listener runs inside
  `on_jvm(...)`, which attaches the polling thread to the Java VM. On every other platform `on_jvm` is a no-op.
- Before a scan, `ble_scan` asks `MainActivity.bluetoothProblem()`: missing permissions are requested (Android 10/11:
  Location; 12+: Nearby devices) and the scan waits for the answer; then Bluetooth must be on and, on Android 11 and
  older, Location must be on. Otherwise the command fails with a plain sentence that the page shows.
- Manifest: `BLUETOOTH`, `BLUETOOTH_ADMIN`, `ACCESS_FINE_LOCATION` (up to API 30), `BLUETOOTH_SCAN`
  (`neverForLocation`) and `BLUETOOTH_CONNECT`.

## Android 10 test phone notes

- Location must be **on** for any Bluetooth scan on Android 10 and 11 (the app says so if it is off).
- The phone's clock was wrong (April 2022) during testing: HTTPS to the account server then fails with certificate
  date errors (`net_error -201`), so the sign-in cannot finish until the clock is right. Its Wi-Fi also had no internet
  (Chrome: `ERR_CONNECTION_RESET` on the sign-in page).
- System WebView 140: fine for the web app.
- Debugging the page: debug builds allow WebView remote debugging:
  `adb forward tcp:9333 localabstract:webview_devtools_remote_$(adb shell pidof app.cavecad.desktop)`, then
  `chrome://inspect` or the DevTools protocol on `localhost:9333`.

## What is verified (2026-11-25, Android 10 phone, USB)

- Debug APK builds and installs; the sign-in gate shows in the phone layout, dark, under the status bar, no white flash.
- Portrait locked on the phone (`mOrientation=1`; before the lock the page came up landscape at 640x360).
- Launcher icon is the CaveCAD logo, label "CaveCAD".
- `adb shell am start -a android.intent.action.VIEW -d 'cavecad://auth?code=x&state=y' app.cavecad.desktop` reaches
  the page's link handler with the running app.
- Sign in opens Chrome on the account server's sign-in page (which could not load: clock and network above).
- Bluetooth: the bridge sets up, the Location prompt appears on the first scan, and after Allow a scan lists the
  nearby BLE devices (50 to 75 devices, names and signal).

## Not verified yet

- The Cavway X1 (`CavwayX1-0312`) was not found: it sleeps after 60 s and needs its button pressed to advertise.
  Connect, services, subscribe and a shot on Android are untested.
- The real Google sign-in round trip (needs the right clock and internet on the phone), and with it the cave picker,
  the tab row and every screen past sign-in, the keyboard, and the light theme's status bar.
- A cold start by a `cavecad://` link (only a link to the running app was tried).
- Tablets (turning), release builds (signing, shrinking).
