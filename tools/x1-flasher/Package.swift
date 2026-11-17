// swift-tools-version:5.9
// X1 Flasher: a small Mac app that flashes Cavway X1 firmware over the USB cable. The flashing itself is the
// `x1flash` command line (tools/cave-x1, Rust, on cave_instrument::cavway_flash); this is the window around it.
import PackageDescription

let package = Package(
    name: "X1Flasher",
    platforms: [.macOS(.v13)],
    targets: [
        .executableTarget(name: "X1Flasher", path: "Sources/X1Flasher"),
    ]
)
