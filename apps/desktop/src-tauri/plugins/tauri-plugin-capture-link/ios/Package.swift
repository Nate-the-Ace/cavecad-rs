// swift-tools-version:5.5
import PackageDescription

let package = Package(
  name: "tauri-plugin-capture-link",
  platforms: [
    .iOS(.v15)
  ],
  products: [
    .library(
      name: "tauri-plugin-capture-link",
      type: .static,
      targets: ["tauri-plugin-capture-link"])
  ],
  dependencies: [
    .package(name: "Tauri", path: "../.tauri/tauri-api")
  ],
  targets: [
    .target(
      name: "tauri-plugin-capture-link",
      dependencies: [
        .byName(name: "Tauri")
      ],
      path: "Sources")
  ]
)
