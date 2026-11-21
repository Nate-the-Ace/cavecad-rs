# Plan: linking CaveCAD Capture and CaveCAD on one iPhone, offline

Repo: /Users/nathanschonegg/Documents/github/cavecad-rs-lidar (branch lidar-capture, HEAD 52048af)

## What I found that shapes the plan

**Capture**
- Stations come from `Core.stations(snapshot:entries:)` (`apps/capture-ios/Sources/Core.swift:54`), which calls `cc_stations` (`apps/capture-ios/rust/src/lib.rs:93-129`), which calls `cave_capture::stations` (`crates/cave-capture/src/stations.rs:90`). Positions are always the adjusted ones.
- Station ids are `cave_id::Id` text. They are the same ids CaveCAD's document uses, so a station id written by CaveCAD matches a shot's `station_id`.
- `StationPickView` keeps `stations` (`StationPickView.swift:45`) and loads them in `loadStations()` (`:203-213`). The saved-shot list and waiting-shot placement both resolve against that same array: `loadSaved()` (`:216-231`), `ShotSummary.stations(in:)` (`ShotView.swift:56-60`) and `StationLabel.find` (`Core.swift:30`).
- So if `stations` is widened to the merged list, waiting shots whose label appears in CaveCAD get placed with no further code.
- `Station` (`Core.swift:4-13`) is `Codable, Hashable` with synthesized conformances, and the Picker tags on it (`StationPickView.swift:256`). Adding a field would change tag equality and Codable decoding. A CaveCAD-only flag is therefore kept as a separate `Set<String>` of ids in the view, not on `Station`.
- `Keychain.swift:10-29` writes without `kSecAttrAccessGroup`. Once a `keychain-access-groups` entitlement exists, items written with no group go to the first group listed. The app's own group must therefore come first, so the token and device key do not land in the shared group.
- `project.yml` has no entitlements yet (`:10-41`). `Resources/Info.plist` has no file-sharing keys.
- Shots live at `Documents/shots/<caveId>/<shotId>.cvcap` (`ShotFiles.swift:6-15`), written with `.completeFileProtectionUntilFirstUserAuthentication`. The thumbnails (`<shotId>.thumb.png`) sit beside them.
- Placement logic lives only in the FFI crate:
  - `place()` (`rust/src/lib.rs:224-250`): compass via `from_compass`, else the heading tap via `heading_from_tap`.
  - `placed_points` (`:252-266`) and `relative_points` (`:361-383`), which subtract the station in f64 before narrowing to f32.
  - The summary JSON (`:280-345`).
- No card-based placement exists at this commit. Cards are only placed relative to the station for display (`:325-342`) and used by `cards_fit` for passes. "Compass or cards" therefore means compass (or an older tap) in this plan. Card-based placement would be new work; flag it to Nathan.
- `cave_capture::decode` copies every JPEG (`bundle.rs:438`, `r.take(..).to_vec()`). A positions-only reader is needed for CaveCAD.
- `check_header` (`bundle.rs:321`) is cheap: it reads the header JSON only.

**CaveCAD**
- The open cave's server id is `engine.roomId()` (wasm `room_id`, `crates/cave-wasm/src/lib.rs:752`). The shelf keeps it per record (`apps/web/src/lib/shelf.svelte.ts:24`, `:147`). It is the same id Capture uses as `caveId`.
- Shelf caves are stored as native ops text. `dxfOf(id)` (`shelf.svelte.ts:116`) and the pure doc-from-ops pattern `room_id_of_native` (`cave-wasm/src/lib.rs:2824-2833`) show how to solve a cave that is not open.
- The open document is `DOC` (`cave-wasm/src/lib.rs:24-26`). cave-wasm does not depend on cave-capture yet (`crates/cave-wasm/Cargo.toml`). cave-capture depends only on cave-doc, cave-id, cave-survey, serde and serde_json, so it is wasm-safe.
- `cave.version` bumps on every edit and `cave.opened` on every open (`apps/web/src/lib/cave.svelte.ts:98-103`).
- `App.svelte:150-180` already hooks `visibilitychange` and starts services in `onMount`.
- The 3D view:
  - Panel: `apps/web/src/components/Viewport3D.svelte`. Its Show menu is at `:651-688`, and `loadPassage` / the re-read effect at `:425-452`.
  - Renderer: `apps/web/src/lib/viewport/cave-view.ts`.
  - `setPassage` (`:1074-1093`) adds a Mesh straight to `this.scene` in cave metres, Z up. `setShowPassage` is at `:1095`.
  - `Scene.stations[].at` (`apps/web/src/lib/engine.ts:52`) is each station as the view currently draws it, following `cave.adjusted`.
- Tauri app (`apps/desktop/src-tauri/src/lib.rs`):
  - On mobile, `folder_from` always returns None (`:85-90`). There is no folder dialog on iOS.
  - Raw-byte responses already use `tauri::ipc::Response` (`:100-104`).
  - App commands are registered in `generate_handler!` (`:268-293`).
  - Path safety lives in `files::safe_join` (`files.rs`). Tests exist there (`files.rs:95`).
  - The crate is outside the engine workspace and already path-depends on a workspace crate, `cave-folder` (`Cargo.toml`). cave-capture can be added the same way.
- The iOS project is tracked: `gen/apple/project.yml` and `gen/apple/cavecad-desktop_iOS/cavecad-desktop_iOS.entitlements`, which is currently empty `<dict/>`. `Info.ios.plist` gets merged in.
- The spec still says App Group (`docs/superpowers/specs/2026-11-17-lidar-room-capture-design.md:68-69`, `:171-176`). It needs correcting.

## Decisions (with reasons)

1. **Keychain write and folder picker: one small local Swift Tauri plugin, `tauri-plugin-capture-link`.**
   - The folder picker needs `UIDocumentPickerViewController` presented from the app's view controller with a delegate. That cannot be done from Rust without hand-rolling an Objective-C class in objc2. A Swift plugin is the supported Tauri route; `tauri-plugin-dialog` follows the same pattern, and it has no bookmark support anyway.
   - Since the plugin is needed anyway, the keychain write goes in it too:
     - The Security calls are line-for-line the same as Capture's `Keychain.swift`: same attributes, one way of doing it in both apps.
     - No `security-framework` / CoreFoundation dictionary code in Rust.
     - Nothing about the keychain call is unit-testable on the Mac anyway, because access groups need the device entitlement.
   - The plugin exposes no JS commands. App commands in `lib.rs` call it through the plugin handle, which keeps the existing `desktop.ts` pattern and needs no capability changes.
2. **Shot decoding: native Rust in src-tauri, not WASM.**
   - Shots are large (JPEG keyframes) and the WebView's WASM heap is 32-bit and copy-heavy.
   - Native code can memory-map the file (`memmap2`), read only header and positions, place and thin them, and return a small float32 buffer. The WebView only ever sees thinned points.
   - Station matching also runs in Rust (one source of truth, `cave_capture::labels_match`).
3. **Stations for the keychain and for placement come from WASM `cave_capture::stations(doc)`.** This is the same adjusted solve Capture uses (CAVECAD-CONTRACT.md row "Station positions").
4. **Display**: placed points are relative to the station (as `relative_points` already does). In the view they are drawn as `THREE.Points` whose object position is the station's displayed `Scene.stations[].at`, so a room stays on its station whether Adjusted is on or off.

## File structure

```
crates/cave-capture/src/
  place.rs            NEW  placement(), relative_points(), station_for()   (moved from capture-ios/rust/src/lib.rs:224-383)
  bundle.rs           +    mesh_positions(bytes) -> (HeaderInfo, Vec<f32>)  (no normals/indices/JPEG copies)
  shared.rs           NEW  SharedStations {v, caveId, name, updatedAt, stations[{id,label,x,y,z}]} + encode (mm rounding)
  lib.rs              +    pub mod place; pub mod shared; re-exports
apps/capture-ios/rust/src/lib.rs   uses cave_capture::place (FFI behaviour unchanged)
crates/cave-wasm/Cargo.toml        + cave-capture
crates/cave-wasm/src/lib.rs        + capture_stations(name, updated_at), capture_stations_of_native(ops, name, updated_at)
apps/web/src/lib/engine.ts         + captureStations(), captureStationsOfNative()
apps/web/src/lib/desktop.ts        + captureShareStations, captureForgetStations, captureLinkState, captureLink, captureShots, captureShotPoints
apps/web/src/lib/capture-share.svelte.ts   NEW  stations -> keychain publisher (open cave debounced, other shelf caves on load)
apps/web/src/lib/capture-shots.svelte.ts   NEW  linked folder state, list + place + fetch points, refresh on open/focus
apps/web/src/lib/capture-pure.ts           NEW  colorFor(shotId), cellFor(budget), cacheKey, hash  (pure, node-testable)
apps/web/tests/captureshots.check.ts       NEW
apps/web/src/lib/viewport/cave-view.ts     + setCaptureRooms(), setShowCaptureRooms()
apps/web/src/components/Viewport3D.svelte  + "LiDAR rooms" section in Show menu (toggle, Link Capture shots…, status)
apps/web/src/App.svelte                    + startCaptureShare(), startCaptureShots()
apps/web/src/lib/shelf.svelte.ts           forgetCave -> captureForgetStations(roomId)
apps/desktop/src-tauri/
  Cargo.toml                       + cave-capture, memmap2, tauri-plugin-capture-link (path)
  src/capture.rs                   NEW  commands + folder resolution + listing + points (tests with temp dirs)
  src/lib.rs                       register plugin + commands
  plugins/tauri-plugin-capture-link/  NEW
    Cargo.toml, build.rs, src/lib.rs (Rust API; no-op off iOS)
    ios/Package.swift, ios/Sources/CaptureLinkPlugin.swift (keychain + picker + bookmark)
  gen/apple/project.yml            entitlements.properties keychain-access-groups
  gen/apple/cavecad-desktop_iOS/cavecad-desktop_iOS.entitlements  same groups
  Info.ios.plist                   + AppIdentifierPrefix = $(AppIdentifierPrefix)
apps/capture-ios/
  project.yml                      + entitlements (own group first, shared second)
  Resources/CaveCADCapture.entitlements  NEW
  Resources/Info.plist             + UIFileSharingEnabled, LSSupportsOpeningDocumentsInPlace, AppIdentifierPrefix
  Sources/Keychain.swift           + explicit own access group on save; SharedStations reader
  Sources/SharedStations.swift     NEW  decode + merge (pure) + read all items
  Sources/StationPickView.swift    merged stations, "from CaveCAD on this phone" marking
  Tests/SharedStationsTests.swift  NEW
  CAVECAD-CONTRACT.md, README.md   new rows / notes
docs/superpowers/specs/2026-11-17-lidar-room-capture-design.md   App Group -> keychain + Files
```

## Tasks

Pure Rust comes first, then CaveCAD's publish side, then Capture's read side (part A, testable on its own). After that the plugin folder link, native shot reading, and the 3D layer (part B).

### Task 1: Move placement into cave-capture, add a positions-only reader and station matching

**Goal:** the placement used by Capture becomes reusable by CaveCAD. Reading a shot for display copies no JPEGs.

**Files:**
- `crates/cave-capture/src/place.rs` (new)
- `crates/cave-capture/src/bundle.rs`
- `crates/cave-capture/src/lib.rs`
- `apps/capture-ios/rust/src/lib.rs`

**Steps:**
1. Write the tests first, in `place.rs` and `bundle.rs` `mod tests`. Reuse the header literals from `rust/src/lib.rs:771`, `:850`, `:1156` (COMPASS) and the waiting-shot test at `:1211`.
   - **Compass shot:** `placement` ignores the target. `relative_points` matches what the FFI test `placed_positions_are_relative_to_the_station` expects.
   - **Tap shot:** with no target it gives `None`; with a target it gives `Some`.
   - **`mesh_positions`:** gives the same positions as `decode(..).positions`. It returns `Truncated` / `BadMesh` / `NotACapture` on the same broken inputs `decode` rejects (non-finite positions, short file).
   - **`station_for`:**
     - A shot with a `station_id` matches by id only (never by label).
     - A waiting shot matches by `labels_match` (`" a 7"` matches `"A7"`).
     - No match gives `None`.
     - For a tap shot, the target is found by `target_station_id`.
2. Implement:
   ```rust
   // place.rs
   pub struct StationRef<'a> { pub id: &'a str, pub label: &'a str, pub pos: Vec3 }
   /// The shot's station (index) and its tap target (index), as Capture resolves them (ShotView.swift:56-60).
   pub fn station_for(h: &ShotHeader, all: &[StationRef]) -> Option<(usize, Option<usize>)> {
       let over = if h.is_waiting() { all.iter().position(|s| labels_match(&h.station_label, s.label))? }
                  else { all.iter().position(|s| s.id == h.station_id)? };
       let target = h.taps.first().and_then(|t| all.iter().position(|s| s.id == t.target_station_id));
       Some((over, target))
   }
   pub fn placement(h: &ShotHeader, station: Vec3, target: Option<Vec3>) -> Option<Placement> { /* body of lib.rs:224-250 */ }
   /// Placed points minus the station, f64 before narrowing (lib.rs:372-381).
   pub fn relative_points(positions: &[f32], p: &Placement, station: Vec3) -> Vec<f32>
   ```
   - `mesh_positions` copies `decode` up to the positions read (`bundle.rs:402-432`). It still checks the whole length (`need`, and the declared JPEG lengths against `bytes.len()`, as `check_header` does) and that the positions are finite. It does not read normals, indices or JPEGs.
3. Switch the FFI's `place`, `placed_points` and `relative_points` to call these. Every existing FFI test must stay green.

**Acceptance:** all old and new tests pass. The FFI output is byte-identical; the existing summary and positions tests prove it.

**Verify:**
```
cargo test -p cave-capture && (cd apps/capture-ios/rust && cargo test)
```

### Task 2: The shared stations format

**Goal:** one Rust definition of the keychain payload that both apps follow.

**Files:**
- `crates/cave-capture/src/shared.rs` (new)
- `crates/cave-capture/tests/fixtures/shared-stations.json` (new)

**Steps:**
1. Tests first:
   - The JSON keys are exactly `v, caveId, name, updatedAt, stations[].{id,label,x,y,z}`.
   - Coordinates are rounded to 1 mm.
   - Encoding the fixture's input reproduces the fixture file byte for byte (the Swift test in Task 5 embeds the same text).
   - Size budget: 5,000 synthetic stations with 26-character ids and 6-character labels stay under 512 KB. Assert it and print the size.
2. Implement:
   ```rust
   #[derive(Serialize, Deserialize)] #[serde(rename_all = "camelCase")]
   pub struct SharedStations { pub v: u32, pub cave_id: String, pub name: String, pub updated_at: i64, pub stations: Vec<SharedStation> }
   #[derive(Serialize, Deserialize)] pub struct SharedStation { pub id: String, pub label: String, pub x: f64, pub y: f64, pub z: f64 }
   pub fn shared_stations(cave_id: &str, name: &str, updated_at: i64, seeds: &[StationSeed]) -> SharedStations // rounds (v*1000).round()/1000
   ```

**Acceptance:** the tests pass and the size is recorded in the test output.

**Verify:**
```
cargo test -p cave-capture shared
```

### Task 3: WASM exposes the solved stations

**Files:**
- `crates/cave-wasm/Cargo.toml` (+ `cave-capture = { path = "../cave-capture" }`)
- `crates/cave-wasm/src/lib.rs`
- `apps/web/src/lib/engine.ts`

**Steps:**
1. Test first, next to `stations_and_sections_come_back_as_json` (`lib.rs:2180`):
   - Call `open_demo(CAVE)`.
   - `capture_stations("Demo", 1)` parses, has `caveId == room_id()`, and every station equals `cave_capture::stations(doc)` (id text, label, position to the mm).
   - `capture_stations_of_native(<ops of the same doc>, ...)` gives the same result.
   - Bad ops give `"null"`.
2. Implement:
   ```rust
   #[wasm_bindgen] pub fn capture_stations(name: &str, updated_at: f64) -> String {
       DOC.with(|d| d.borrow().as_ref().map(|doc| {
           let s = cave_capture::stations(doc);
           serde_json::to_string(&cave_capture::shared::shared_stations(&cave_draw::room_id(doc), name, updated_at as i64, &s)).unwrap()
       }).unwrap_or_else(|| "null".into()))
   }
   // capture_stations_of_native: as room_id_of_native (lib.rs:2824) then the same.
   ```
3. Add the wrappers to `engine.ts`, next to `roomId`:
   - `captureStations(name, updatedAt): SharedStations | null`
   - `captureStationsOfNative(ops, name, updatedAt)`
   - Also return the raw text, so it is not stringified twice.

**Verify:**
```
cargo test -p cave-wasm capture_
(cd apps/web && npm run build)
```

### Task 4: Tauri plugin skeleton plus keychain write and delete (CaveCAD side of A)

**Goal:** CaveCAD on iOS can write and delete `cavecad.stations` items in `<prefix>app.cavecad.shared`.

**Files:**
- `apps/desktop/src-tauri/plugins/tauri-plugin-capture-link/*`
- `src-tauri/Cargo.toml`
- `src-tauri/src/lib.rs`
- `src-tauri/src/capture.rs`
- `gen/apple/project.yml`
- `gen/apple/cavecad-desktop_iOS/cavecad-desktop_iOS.entitlements`
- `Info.ios.plist`
- `apps/web/src/lib/desktop.ts`

**Steps:**
1. Plugin crate, the standard Tauri 2 mobile plugin layout:
   ```toml
   # Cargo.toml
   [dependencies] tauri = "2"  serde = {version="1",features=["derive"]}  serde_json = "1"
   [build-dependencies] tauri-plugin = { version = "2", features = ["build"] }
   ```
   ```rust
   // build.rs
   fn main() { tauri_plugin::Builder::new(&[]).ios_path("ios").build(); }
   // src/lib.rs
   #[cfg(target_os = "ios")] tauri::ios_plugin_binding!(init_plugin_capture_link);
   pub struct CaptureLink<R: Runtime>(#[allow(dead_code)] Option<tauri::plugin::PluginHandle<R>>);
   impl<R: Runtime> CaptureLink<R> {
       pub fn write_stations(&self, cave_id: &str, json: &str) -> Result<(), String> { self.call("writeStations", json!({"caveId":cave_id,"json":json})).map(|_: serde_json::Value| ()) }
       pub fn delete_stations(&self, cave_id: &str) -> Result<(), String> { ... "deleteStations" }
       pub fn pick_folder(&self) -> Result<Option<String>, String> { ... "pickFolder" -> {"path": string|null} }
       pub fn resolve_folder(&self) -> Result<Resolved, String> { ... "resolveFolder" -> {"path": string|null, "stale": bool} }
       fn call<T: DeserializeOwned>(&self, cmd: &str, a: serde_json::Value) -> Result<T, String> {
           match &self.0 { Some(h) => h.run_mobile_plugin(cmd, a).map_err(|e| e.to_string()), None => Err("only in CaveCAD on iPhone and iPad".into()) }
       }
   }
   pub fn init<R: Runtime>() -> TauriPlugin<R> {
       Builder::new("capture-link").setup(|app, _api| {
           #[cfg(target_os = "ios")] let h = Some(_api.register_ios_plugin(init_plugin_capture_link)?);
           #[cfg(not(target_os = "ios"))] let h = None;
           app.manage(CaptureLink::<R>(h)); Ok(())
       }).build()
   }
   ```
   - `ios/Package.swift` depends on `.package(name: "Tauri", path: "../.tauri/tauri-api")`, the path `tauri plugin new` generates. Check it against the tauri version in `Cargo.lock`.
2. `CaptureLinkPlugin.swift`, keychain part:
   ```swift
   import Tauri; import Security; import UIKit; import UniformTypeIdentifiers
   struct StationsArgs: Decodable { let caveId: String; let json: String }
   struct CaveArgs: Decodable { let caveId: String }
   class CaptureLinkPlugin: Plugin {
     static let service = "cavecad.stations"
     /// "<TeamID>." from Info.plist (AppIdentifierPrefix = $(AppIdentifierPrefix)), fallback for safety.
     static var group: String { ((Bundle.main.object(forInfoDictionaryKey: "AppIdentifierPrefix") as? String) ?? "2ANLK72UUS.") + "app.cavecad.shared" }
     static func base(_ cave: String) -> [String: Any] {
       [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: service,
        kSecAttrAccount as String: cave, kSecAttrAccessGroup as String: group,
        kSecAttrSynchronizable as String: false]
     }
     @objc public func writeStations(_ invoke: Invoke) throws {
       let a = try invoke.parseArgs(StationsArgs.self)
       let data = Data(a.json.utf8)
       var st = SecItemUpdate(Self.base(a.caveId) as CFDictionary, [kSecValueData as String: data] as CFDictionary)
       if st == errSecItemNotFound {
         var add = Self.base(a.caveId); add[kSecValueData as String] = data
         add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
         st = SecItemAdd(add as CFDictionary, nil)
       }
       st == errSecSuccess ? invoke.resolve(["bytes": data.count]) : invoke.reject("keychain status \(st)")
     }
     @objc public func deleteStations(_ invoke: Invoke) throws { let a = try invoke.parseArgs(CaveArgs.self); SecItemDelete(Self.base(a.caveId) as CFDictionary); invoke.resolve() }
   }
   @_cdecl("init_plugin_capture_link") func initPlugin() -> Plugin { CaptureLinkPlugin() }
   ```
3. Entitlements, in both `project.yml` (`entitlements: path: ... properties: keychain-access-groups: [...]`, so a regenerate keeps them) and the tracked `.entitlements` file:
   ```xml
   <key>keychain-access-groups</key><array>
     <string>$(AppIdentifierPrefix)app.cavecad.desktop</string>   <!-- own group FIRST: the default for unscoped items -->
     <string>$(AppIdentifierPrefix)app.cavecad.shared</string>
   </array>
   ```
   - Add `<key>AppIdentifierPrefix</key><string>$(AppIdentifierPrefix)</string>` to `Info.ios.plist`.
4. `src/capture.rs` commands, registered in `lib.rs:268`, with `.plugin(tauri_plugin_capture_link::init())`:
   - `capture_share_stations(app, cave_id, json)`:
     - Validate `cave_id` with `^[A-Za-z0-9_-]{1,64}$` (a pure `fn valid_cave_id`, tested).
     - Reject `json.len() > 1_000_000`.
     - Skip if the SHA/FNV hash equals the last one written (`State<Mutex<HashMap<String,u64>>>`).
     - Call `write_stations`.
   - `capture_forget_stations(app, cave_id)`.
   - `capture_link_available() -> bool { cfg!(target_os = "ios") }`.
5. `desktop.ts` wrappers, in the style of `:28-58`.

**Acceptance:**
- Desktop `cargo build` and tests pass; on desktop the plugin is a no-op.
- `tauri ios build` links the Swift package.
- On the phone, the first write returns `{bytes:n}` (checked in Task 5's device step).

**Verify:**
```
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml capture
(cd apps/desktop && npx tauri ios build --debug --target aarch64)
```

### Task 5: The publisher in the web app

**Files:**
- `apps/web/src/lib/capture-share.svelte.ts` (new)
- `apps/web/src/App.svelte`
- `apps/web/src/lib/shelf.svelte.ts`

**Steps:**
1. `startCaptureShare()` does nothing unless `await captureLinkAvailable()`.
2. Open cave:
   ```ts
   $effect.root(() => $effect(() => { void cave.version; void cave.opened; untrack(schedule); }));
   function schedule() { clearTimeout(t); t = setTimeout(publishOpen, cave.opened !== lastOpened ? 0 : 2000); }
   async function publishOpen() {
     const id = engine.roomId(); if (!cave.hasCave || !id) return;          // samples have no room (room.svelte.ts header)
     const text = engine.captureStationsText(cave.name, Date.now());
     if (text !== 'null') await captureShareStations(id, text).catch((e) => engine.log('warn', 'capture', String(e)));
   }
   ```
   `updatedAt` should be the time of the last change, not `Date.now()` at every call. Keep `lastChangeAt` updated in the effect.
3. Other offline caves: after `loadShelf()` (`App.svelte:166`), go through `shelf.records` with `roomId`, one per `requestIdleCallback`/`setTimeout(…,50)`:
   ```ts
   const text = await dxfOf(r.id);
   if (text && isNative(text)) {
     captureShareStations(r.roomId, engine.captureStationsOfNativeText(text, r.name, r.updated));
   }
   ```
   Rust skips unchanged ones by hash.
4. `forgetCave` (`shelf.svelte.ts:233`) calls `captureForgetStations(rec.roomId)` when set.

**Acceptance:** after opening a cave and editing it, the console shows one `capture` info line per write (log `n stations, bytes`). The device check comes in Task 6.

**Verify:**
```
(cd apps/web && npm run check && npm run build)
```

### Task 6: Capture reads and merges the shared stations (Capture side of A)

**Files:**
- `apps/capture-ios/project.yml`
- `Resources/CaveCADCapture.entitlements` (new)
- `Resources/Info.plist`
- `Sources/Keychain.swift`
- `Sources/SharedStations.swift` (new)
- `Sources/StationPickView.swift`
- `Tests/SharedStationsTests.swift` (new)

**Steps:**
1. Tests first, in `SharedStationsTests.swift`:
   - **decode:** the fixture text from Task 2 is embedded as a literal. Add a comment that it must equal `crates/cave-capture/tests/fixtures/shared-stations.json`.
   - **merge(kept:shared:keptAt:):**
     - A station only CaveCAD has is appended and its id is in `fromCaveCAD`.
     - For the same id: if `shared.updatedAt > keptAt`, the shared position wins; otherwise the kept one.
     - Order is kept stations first, then CaveCAD-only ones in their order.
   - **waiting shot:** with typed label "A9" not in the kept list but in the shared list, `StationLabel.find("a 9", in: merged)` returns the CaveCAD station. This mirrors `WaitingStationTests.swift`, and the `ShotSummary.stations(in:)` path from `ShotView.swift:56` then places the shot.
2. Implement:
   ```swift
   struct SharedStations: Decodable { let v: Int; let caveId: String; let name: String; let updatedAt: Int64; let stations: [Station] }  // Station's Codable keys already id,label,x,y,z
   enum SharedStore {
     static func all() -> [SharedStations] {
       let q: [String: Any] = [kSecClass as String: kSecClassGenericPassword, kSecAttrService as String: "cavecad.stations",
                               kSecAttrAccessGroup as String: Keychain.sharedGroup, kSecMatchLimit as String: kSecMatchLimitAll,
                               kSecReturnData as String: true, kSecReturnAttributes as String: true]
       var out: CFTypeRef?; guard SecItemCopyMatching(q as CFDictionary, &out) == errSecSuccess,
             let items = out as? [[String: Any]] else { return [] }
       return items.compactMap { ($0[kSecValueData as String] as? Data).flatMap { try? JSONDecoder().decode(SharedStations.self, from: $0) } }
     }
     static func of(cave: String) -> SharedStations? { /* same with kSecAttrAccount: cave, kSecMatchLimitOne */ }
     static func merge(kept: [Station], shared: SharedStations?, keptAt: Date) -> (stations: [Station], fromCaveCAD: Set<String>)
   }
   ```
   - In `Keychain.swift`, `save` now sets `kSecAttrAccessGroup = ownGroup`, where `ownGroup = prefix + "app.cavecad.capture"`. Existing items are still found, because `load` does not filter by group.
3. Entitlements, own group first:
   ```xml
   <key>keychain-access-groups</key><array>
     <string>$(AppIdentifierPrefix)app.cavecad.capture</string>
     <string>$(AppIdentifierPrefix)app.cavecad.shared</string></array>
   ```
   - In `project.yml`, under the `CaveCADCapture` target: `entitlements: { path: Resources/CaveCADCapture.entitlements, properties: { keychain-access-groups: [...] } }`.
   - In `Info.plist`: `AppIdentifierPrefix`, plus (used in Task 7) `<key>UIFileSharingEnabled</key><true/><key>LSSupportsOpeningDocumentsInPlace</key><true/>`.
4. In `StationPickView.loadStations()` (`:203-213`), after `Core.stations`:
   ```swift
   let shared = SharedStore.of(cave: cave.id)
   (stations, fromCaveCAD) = SharedStore.merge(kept: s, shared: shared, keptAt: cave.keptAt)
   Log.write("stations", "\(s.count) kept + \(fromCaveCAD.count) from CaveCAD on this phone")
   ```
   - Re-run it on `scenePhase == .active`, so a CaveCAD edit shows after switching apps.
   - In the picker (`:256`), CaveCAD-only stations get a `Text("\($0.label) · from CaveCAD on this phone")` row.
   - Add a footer when `!fromCaveCAD.isEmpty`.
5. Optional, small: in `CaveStore.refresh()` offline (`CaveStore.swift:88-91`), also list keychain caves that are not kept, as `KeptCave(id:, name:, snapshot: nil, entries: [], last: 0, keptAt: .distantPast)`, so a cave only in CaveCAD can be captured in. `Core.stations(nil, [])` returns `[]`, and the merge supplies the stations.

**Acceptance:**
- Unit tests pass.
- On the phone, `test-cmd.sh <udid> 'action=station&label=<label only in CaveCAD>'` logs `[test] station ok: over <label>` (not "waiting"). The test-command path refuses labels not in `stations` (`StationPickView.swift:185-189`), so this is a real check.
- A shot previously saved waiting for that label logs `waiting shot <id> placed at <label>` (`StationPickView.swift:34`).

**Verify:**
```
cd apps/capture-ios && ./rust/build-xcframework.sh && xcodegen generate && xcodebuild test -scheme CaveCADCapture -destination 'platform=iOS Simulator,name=iPhone 18 Pro' CODE_SIGNING_ALLOWED=NO
```
In the simulator the keychain access group is not entitled, so only the pure tests run.

### Task 7: Linking Capture's folder: picker, bookmark, resolve (CaveCAD side of B)

**Files:**
- `CaptureLinkPlugin.swift`
- plugin `src/lib.rs` (already sketched)
- `src-tauri/src/capture.rs`
- `desktop.ts`

**Steps:**
1. Swift:
   ```swift
   private var pending: Invoke?; private var held: URL?
   static let key = "captureFolderBookmark"
   @objc public func pickFolder(_ invoke: Invoke) {
     DispatchQueue.main.async {
       let p = UIDocumentPickerViewController(forOpeningContentTypes: [.folder])
       p.delegate = self; p.allowsMultipleSelection = false; self.pending = invoke
       self.manager.viewController?.present(p, animated: true)
     }
   }
   func documentPicker(_ c: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
     guard let u = urls.first, u.startAccessingSecurityScopedResource() else { pending?.reject("no access"); return }
     do { UserDefaults.standard.set(try u.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo: nil), forKey: Self.key)
          hold(u); pending?.resolve(["path": u.path]) } catch { pending?.reject("\(error)") }
     pending = nil
   }
   func documentPickerWasCancelled(_ c: UIDocumentPickerViewController) { pending?.resolve(["path": NSNull()]); pending = nil }
   @objc public func resolveFolder(_ invoke: Invoke) {
     guard let b = UserDefaults.standard.data(forKey: Self.key) else { invoke.resolve(["path": NSNull(), "stale": false]); return }
     var stale = false
     guard let u = try? URL(resolvingBookmarkData: b, options: [], relativeTo: nil, bookmarkDataIsStale: &stale),
           u.startAccessingSecurityScopedResource() else { invoke.resolve(["path": NSNull(), "stale": true]); return }
     if stale, let nb = try? u.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo: nil) { UserDefaults.standard.set(nb, forKey: Self.key) }
     hold(u); invoke.resolve(["path": u.path, "stale": stale])
   }
   private func hold(_ u: URL) { if let h = held, h != u { h.stopAccessingSecurityScopedResource() }; held = u }  // kept for the app's life
   ```
   - The class adopts `UIDocumentPickerDelegate`.
   - iOS bookmarks have no `.withSecurityScope` option (macOS only); a picked URL's bookmark carries its scope implicitly.
   - Access stays held so Rust's `std::fs` can read the path in the same process.
2. `capture.rs`:
   - `struct Linked(Mutex<Option<PathBuf>>)`.
   - `capture_link(app)` calls `pick_folder`, then `shots_root(path)`, and stores the result.
   - `capture_link_state(app)` calls `resolve_folder` (every app focus) and returns `{linked, stale, problem}`.
   - Pure helper, tested:
   ```rust
   /// The folder the person picked is Capture's Documents ("Capture" in Files) or its "shots" folder.
   pub fn shots_root(picked: &Path) -> Option<PathBuf> {
       if picked.file_name().is_some_and(|n| n == "shots") && picked.is_dir() { return Some(picked.into()) }
       let s = picked.join("shots"); s.is_dir().then_some(s)
   }
   ```

**Acceptance:**
- In CaveCAD: Show, then LiDAR rooms, then "Link Capture shots…" opens Files.
- Picking "On My iPhone › Capture" gives `linked: true`.
- After force-quitting CaveCAD and reopening, `capture_link_state` is still `linked` with no picker.
- Picking an unrelated folder gives "That folder has no shots folder: pick Capture's folder".

**Verify:**
```
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml shots_root
```
Plus the device steps below.

### Task 8: Native listing, placing and points

**Files:**
- `src-tauri/src/capture.rs`
- `src-tauri/Cargo.toml` (+ `cave-capture = { path = "../../../crates/cave-capture" }`, `memmap2 = "0.9"`)

**Steps:**
1. Tests first. Use a temp dir; tests write shots with `cave_capture::encode` and the compass header from Task 1.
   - `list_shots(root, "cave1", &stations)` gives one entry per `.cvcap`, ignores `.thumb.png`, sorts by `captured_at_ms`, and gives `status = placed | waiting | gone | unreadable` (`unreadable` for a junk file).
   - A cave id of `../x` is refused.
   - `shot_points(root, cave, file, pair, cell)` returns `len % 12 == 0`, and the result equals `thin_points(relative_points(..), cell)`.
   - A file name with `/` or a name not ending `.cvcap` is refused (reuse `files::safe_join`).
2. Implement:
   ```rust
   #[derive(Deserialize)] pub struct St { id: String, label: String, x: f64, y: f64, z: f64 }
   #[derive(Serialize)] pub struct ShotRow { file: String, shot_id: String, label: String, captured_at_ms: i64, modified_ms: u64,
       status: &'static str, station_id: Option<String>, pair: Option<[f64; 6]>, vertices: usize }
   #[tauri::command] async fn capture_shots(linked: State<'_, Linked>, cave_id: String, stations: Vec<St>) -> Result<Vec<ShotRow>, String>
   // per file: mmap -> check_header -> header.cave_id == cave_id? -> station_for -> pair = [over xyz, target xyz or over xyz]
   //           -> status: placed if placement(..).is_some(), waiting if header.is_waiting() && none, else gone
   #[tauri::command] async fn capture_shot_points(linked: State<'_, Linked>, cave_id: String, file: String, pair: [f64; 6], cell_m: f32) -> Result<Response, String>
   // mmap -> mesh_positions -> placement -> relative_points -> thin_points(cell_m) -> le bytes -> Response::new
   ```
   - Run both commands in `tauri::async_runtime::spawn_blocking`.
   - The station list passed in is `engine.captureStations().stations`, so ids and positions are exactly Capture's.

**Verify:**
```
cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml capture
```

### Task 9: Web shot state, refresh and budget

**Files:**
- `apps/web/src/lib/capture-pure.ts`
- `apps/web/src/lib/capture-shots.svelte.ts`
- `apps/web/tests/captureshots.check.ts`
- `apps/web/package.json` (`test:captureshots`, appended to `test`)

**Steps:**
1. Pure, tested first:
   - `colorFor(shotId)`: FNV hash, hue by golden angle, s 0.65, l 0.55. Stable and distinct for consecutive ids.
   - `cellFor(totalVertices, budget = 1_500_000)`: start at 0.08 m and double until the estimate fits. Estimate = vertices × (0.02 / cell)², clamped.
   - `pointsKey(row)` = `file|modified_ms|pair rounded to mm|cell`.
2. State:
   ```ts
   export const captureShots = $state({ available: false, linked: false, stale: false, problem: '', rows: [] as ShotRow[],
                                         layers: [] as { key: string; station: string; points: Float32Array; color: number }[] });
   ```
   - `refresh()` runs on `cave.opened`, on `visibilitychange` to visible, on window `focus`, and when the stations text from Task 5 changes. It:
     1. calls `captureLinkState()`;
     2. if linked and `engine.roomId()`, calls `captureShots(id, stations)`;
     3. for each `placed` row, fetches `captureShotPoints` only when `pointsKey` is not in the cache, and drops cache entries whose rows are gone;
     4. builds `layers`.
   - Do one request at a time. Log `n placed, m waiting, k gone` once per change.

**Verify:**
```
(cd apps/web && npm run test:captureshots && npm run check)
```

### Task 10: The "LiDAR rooms" layer in the 3D view

**Files:**
- `apps/web/src/lib/viewport/cave-view.ts`
- `apps/web/src/components/Viewport3D.svelte`

**Steps:**
1. In `cave-view.ts`, next to `setPassage` (`:1074`):
   ```ts
   private captureGroup = new THREE.Group(); private showCapture = true;   // added to this.scene in the constructor (:235)
   setCaptureRooms(list: { key: string; at: [number, number, number]; points: Float32Array; color: number }[]) {
     for (const c of [...this.captureGroup.children]) { const p = c as THREE.Points; this.captureGroup.remove(p); p.geometry.dispose(); (p.material as THREE.Material).dispose(); }
     for (const r of list) {
       const g = new THREE.BufferGeometry(); g.setAttribute('position', new THREE.BufferAttribute(r.points, 3));
       const m = new THREE.PointsMaterial({ color: r.color, size: 2.5, sizeAttenuation: false });
       const p = new THREE.Points(g, m); p.position.set(...r.at); p.name = r.key; p.frustumCulled = true; this.captureGroup.add(p);
     }
     this.captureGroup.visible = this.showCapture; this.requestRender();
   }
   setShowCaptureRooms(on: boolean) { this.showCapture = on; this.captureGroup.visible = on; this.requestRender(); }
   ```
   - The geometries are rebuilt only for keys that changed; diff by `key`, as `scanWanted` does at `:837`.
   - `fit()` ignores the group, so framing stays on the survey.
2. In `Viewport3D.svelte`, an effect on `captureShots.layers`, `cave.version` and `cave.adjusted` maps each layer's `station` to `scene.stations.find(s => s.id === station)?.at` and calls `view.setCaptureRooms`.
3. In the Show menu (`:657-668`), when `captureShots.available`, add a `<h4>LiDAR rooms</h4>` section:
   - a checkbox bound to `option('captureRooms', true)`;
   - the status line `{placed} placed · {waiting} waiting for station · {gone} station gone`;
   - a button "Link Capture shots…" (or "Link again…" when `stale`/`!linked`) calling `captureLink()` then `refresh()`;
   - the note "Read from CaveCAD Capture on this phone. Nothing is uploaded or saved into the cave."

**Acceptance:**
- With linked shots, coloured point clouds sit at their stations.
- Toggling Adjusted moves them with their stations.
- The checkbox hides them.
- Editing the survey (moving a station) moves its room after the debounce.

**Verify:**
```
(cd apps/web && npm run check && npm run build)
```
Plus the device steps below.

### Task 11: Contracts and docs

1. `apps/capture-ios/CAVECAD-CONTRACT.md`: add rows.
   - "Shared stations: keychain generic password, service `cavecad.stations`, account = cave id, group `<prefix>app.cavecad.shared`, JSON `{v,caveId,name,updatedAt,stations[{id,label,x,y,z}]}` adjusted to mm" | owner `crates/cave-capture/src/shared.rs`, `apps/web/src/lib/capture-share.svelte.ts`, `CaptureLinkPlugin.swift` | Capture code `SharedStations.swift`.
   - Add the reverse promise to a new section, "What CaveCAD relies on in Capture": `Documents/shots/<caveId>/<shotId>.cvcap`, exposed in Files; owner `ShotFiles.swift`, `Info.plist`.
2. Spec (`docs/superpowers/specs/2026-11-17-lidar-room-capture-design.md:68-69`, `:171-176`): replace "App Group" with keychain plus the Files folder link. Reason: a Personal Team has no App Groups. Shots stay read-only in CaveCAD until step 5.
3. `apps/capture-ios/README.md`: mention the Files exposure and that `capture.log` now shows in Files.

## Checking it on the phone (00008150-0011281122D2401C, over Wi-Fi)

Before starting:
- The phone is paired with "Connect via network" in Xcode, unlocked, and on the same Wi-Fi.
- `xcrun devicectl list devices` shows it as available (network).
- `install.sh` and `ipad-install.sh` both use `devicectl` and `xcodebuild -destination id=…`, which work over the network once paired.
- `live-log.sh` uses `idevicesyslog -n` (network).

1. Install both:
   ```
   apps/capture-ios/install.sh 2ANLK72UUS 00008150-0011281122D2401C
   tools/ipad-install.sh 2ANLK72UUS 00008150-0011281122D2401C
   ```
   - First time with the new entitlements: if `xcodebuild` reports that the profile lacks `keychain-access-groups`, run it once with `-allowProvisioningUpdates`. Both scripts already pass it.
2. **Part A:**
   - In CaveCAD, open a cave that has a room, go offline (airplane mode), and add a new station, for example `Z1` via the notebook.
   - The Console shows `capture: … stations written (… bytes)`.
   - Switch to Capture with `apps/capture-ios/live-log.sh 00008150-0011281122D2401C` running.
   - Open the same cave: `test-cmd.sh <udid> 'action=open&cave=<name>'`. The log shows `[stations] N kept + 1 from CaveCAD on this phone`.
   - Run `test-cmd.sh <udid> 'action=station&label=Z1'`. Expect `[test] … ok: over Z1`, not "waiting" and not refused.
   - Earlier waiting shot: in Capture take a shot "Other station…" `Z2` (or `'action=station&label=Z2&other=1'`). Then add `Z2` in CaveCAD, return to Capture, and expect `[shots] waiting shot <id> placed at Z2`.
3. **Part B:**
   - In Files, "On My iPhone" now lists "Capture" with `shots/`.
   - In CaveCAD: Map, Show, LiDAR rooms, "Link Capture shots…", then pick "Capture".
   - The status shows the placed count and the rooms appear on their stations.
   - Force-quit and reopen CaveCAD: still linked, rooms come back with no picker.
   - Take a new shot in Capture and switch back to CaveCAD: the new room appears (refresh on focus).
   - Toggle Adjusted and the checkbox.
   - Memory: with roughly 20 shots open, watch Xcode's Debug Navigator. Attach to CaveCAD with Debug › Attach to Process by PID or name. Check the WebContent process stays under about 300 MB.

## Risks

- **Keychain item size.**
  - No documented per-item limit, but the keychain is meant for small items, and large ones cost securityd memory.
  - Task 2 measures it (target under 256 KB for real caves; around 70 B per station means about 3,500 stations).
  - Fallback if a real cave exceeds 512 KB: split into `account = caveId`, holding `{…, parts: n}` plus the first part, and `caveId#1..n`. Capture's reader concatenates the station arrays. Keep the `v` field so that change is versioned.
  - Write only on change (hash) and debounce at 2 s, so keychain churn stays low.
- **Default access group.** Adding `keychain-access-groups` moves unscoped writes into the first listed group. The plan lists each app's own group first and makes Capture's `Keychain.save` explicit. Check that the token and device key still load after upgrade; `load` is not group-filtered, so they do.
- **Personal Team.** Profiles are re-signed every 7 days.
  - If Capture expires and is deleted rather than reinstalled over the top, its container (and shots) go, and the bookmark becomes unresolvable. The UI shows "Link again".
  - Keychain items survive app reinstall on iOS, but not "Erase".
  - Both apps must be signed by the same team for the shared group, which they are.
- **Bookmark staleness and access.**
  - `resolveFolder` re-saves stale bookmarks.
  - Access is held for the app's lifetime. If iOS ever revokes it, the next focus re-resolves.
  - Direct POSIX reads from Rust work for "On My iPhone" app folders. If a device shows permission errors anyway, the fallback is the Swift plugin reading the bytes with `NSFileCoordinator` and copying the `.cvcap` files into CaveCAD's Caches, keyed by name and mtime, with Rust reading from there.
  - The shot files are protected until first unlock, so reading after unlock is fine.
- **Tauri iOS plugin effort.**
  - First local Swift plugin in this repo. Needs the `tauri-plugin` build feature, the Swift package path matching the Tauri version, and `manager.viewController` for presenting.
  - Budget most of Task 4 for getting `tauri ios build` to link it; desktop builds must stay unaffected (no-op `None` handle).
  - `tauri ios init` would regenerate `gen/apple`. Entitlements live in both `project.yml` `properties` and the tracked file so a regenerate keeps them.
- **WebView memory with many shots.**
  - Only thinned, relative float32 points cross IPC.
  - The global budget (`cellFor`) caps the total at about 1.5M points (18 MB).
  - Geometries are disposed on replace, and the points cache is dropped for rows that are gone or changed.
  - The native side maps files and copies only positions (`mesh_positions`).
  - If iPhone memory warnings appear, lower the budget, or later switch to `passes::ghost` simplified meshes.
- **Placement consistency.** Placement uses the adjusted solve (same as Capture). Display offsets use the station as the view shows it, so rooms ride their stations when Adjusted is off. Their internal tap-shot heading is still from the adjusted solve, which is what Capture shows.
- **"Cards" placement** does not exist in code at this commit (`rust/src/lib.rs:224-250` is compass or tap only). If Nathan meant card-based alignment, that is a separate design item.

### Critical files for implementation
- /Users/nathanschonegg/Documents/github/cavecad-rs-lidar/apps/capture-ios/rust/src/lib.rs (placement to move, lines 224-383)
- /Users/nathanschonegg/Documents/github/cavecad-rs-lidar/apps/capture-ios/Sources/StationPickView.swift (merge point, lines 203-231 and 256)
- /Users/nathanschonegg/Documents/github/cavecad-rs-lidar/apps/desktop/src-tauri/src/lib.rs (plugin and commands registration, lines 233-295)
- /Users/nathanschonegg/Documents/github/cavecad-rs-lidar/apps/web/src/lib/viewport/cave-view.ts (layer next to setPassage, line 1074) and /Users/nathanschonegg/Documents/github/cavecad-rs-lidar/apps/web/src/components/Viewport3D.svelte (Show menu, lines 651-688)
- /Users/nathanschonegg/Documents/github/cavecad-rs-lidar/crates/cave-wasm/src/lib.rs (capture_stations next to room_id at line 752 and room_id_of_native at line 2824)

