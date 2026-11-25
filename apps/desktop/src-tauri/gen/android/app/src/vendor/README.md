# Vendored Java for Bluetooth on Android

The Rust `btleplug` crate (src-tauri/src/ble.rs) talks to Android's Bluetooth through these Java classes. They ship
inside the Rust crates but not as an Android library, so they are copied here unchanged (app/build.gradle.kts adds
`src/vendor/java` to the app's sources):

- `com/nonpolynomial/btleplug/android/impl/*`: btleplug 0.11.8, `src/droidplug/java/src/main/java` (LICENSE-btleplug.md)
- `io/github/gedgygedgy/rust/*`: jni-utils 0.1.1, `java/src/main/java` (LICENSE-jni-utils)

When Cargo.lock moves btleplug or jni-utils to another version, copy the new files from
`~/.cargo/registry/src/*/btleplug-<v>/src/droidplug/java/src/main/java` and `jni-utils-<v>/java/src/main/java`.
