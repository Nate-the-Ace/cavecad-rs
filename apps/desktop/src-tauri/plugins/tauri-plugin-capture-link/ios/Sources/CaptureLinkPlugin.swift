// CaveCAD <-> CaveCAD Capture on one iPhone or iPad, offline.
// - Stations: one keychain generic password per cave in the shared access group <team>.app.cavecad.shared
//   (service "cavecad.stations", account = server cave id, JSON from crates/cave-capture/src/shared.rs).
//   Same attributes as Capture's Keychain.swift; never synchronised, this device only.
// - Shots: Capture's Documents folder ("Capture" in Files), picked once with the Files picker and remembered
//   as a bookmark; access is held for the app's life so Rust can read the files by path.

import Foundation
import Security
import Tauri
import UIKit
import UniformTypeIdentifiers

struct StationsArgs: Decodable {
  let caveId: String
  let json: String
}

struct CaveArgs: Decodable {
  let caveId: String
}

class CaptureLinkPlugin: Plugin, UIDocumentPickerDelegate {
  static let service = "cavecad.stations"
  static let bookmarkKey = "captureFolderBookmark"

  /// "<TeamID>." from Info.plist (AppIdentifierPrefix = $(AppIdentifierPrefix)).
  static var prefix: String {
    (Bundle.main.object(forInfoDictionaryKey: "AppIdentifierPrefix") as? String) ?? "2ANLK72UUS."
  }
  static var group: String { prefix + "app.cavecad.shared" }

  static func base(_ cave: String) -> [String: Any] {
    [
      kSecClass as String: kSecClassGenericPassword,
      kSecAttrService as String: service,
      kSecAttrAccount as String: cave,
      kSecAttrAccessGroup as String: group,
      kSecAttrSynchronizable as String: false,
    ]
  }

  private var pending: Invoke?
  private var held: URL?

  @objc public func writeStations(_ invoke: Invoke) throws {
    let a = try invoke.parseArgs(StationsArgs.self)
    let data = Data(a.json.utf8)
    var st = SecItemUpdate(
      Self.base(a.caveId) as CFDictionary, [kSecValueData as String: data] as CFDictionary)
    if st == errSecItemNotFound {
      var add = Self.base(a.caveId)
      add[kSecValueData as String] = data
      add[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
      st = SecItemAdd(add as CFDictionary, nil)
    }
    if st == errSecSuccess {
      invoke.resolve(["bytes": data.count])
    } else {
      invoke.reject("keychain status \(st) (group \(Self.group))")
    }
  }

  @objc public func deleteStations(_ invoke: Invoke) throws {
    let a = try invoke.parseArgs(CaveArgs.self)
    let st = SecItemDelete(Self.base(a.caveId) as CFDictionary)
    if st == errSecSuccess || st == errSecItemNotFound {
      invoke.resolve([:])
    } else {
      invoke.reject("keychain status \(st)")
    }
  }

  @objc public func pickFolder(_ invoke: Invoke) {
    DispatchQueue.main.async {
      if let old = self.pending {
        old.resolve(["path": NSNull()])
      }
      let p = UIDocumentPickerViewController(forOpeningContentTypes: [.folder])
      p.delegate = self
      p.allowsMultipleSelection = false
      self.pending = invoke
      guard let vc = self.manager.viewController else {
        self.pending = nil
        invoke.reject("no window to show the folder picker in")
        return
      }
      vc.present(p, animated: true)
    }
  }

  public func documentPicker(
    _ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]
  ) {
    guard let invoke = pending else { return }
    pending = nil
    guard let u = urls.first else {
      invoke.resolve(["path": NSNull()])
      return
    }
    guard u.startAccessingSecurityScopedResource() else {
      invoke.reject("CaveCAD was not given access to that folder")
      return
    }
    do {
      let b = try u.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo: nil)
      UserDefaults.standard.set(b, forKey: Self.bookmarkKey)
      hold(u)
      invoke.resolve(["path": u.path])
    } catch {
      u.stopAccessingSecurityScopedResource()
      invoke.reject("could not remember that folder: \(error.localizedDescription)")
    }
  }

  public func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
    pending?.resolve(["path": NSNull()])
    pending = nil
  }

  @objc public func resolveFolder(_ invoke: Invoke) {
    guard let b = UserDefaults.standard.data(forKey: Self.bookmarkKey) else {
      invoke.resolve(["path": NSNull(), "stale": false])
      return
    }
    var stale = false
    guard
      let u = try? URL(
        resolvingBookmarkData: b, options: [], relativeTo: nil, bookmarkDataIsStale: &stale)
    else {
      invoke.resolve(["path": NSNull(), "stale": true])
      return
    }
    if held != u {
      guard u.startAccessingSecurityScopedResource() else {
        invoke.resolve(["path": NSNull(), "stale": true])
        return
      }
      hold(u)
    }
    if stale,
      let nb = try? u.bookmarkData(options: [], includingResourceValuesForKeys: nil, relativeTo: nil)
    {
      UserDefaults.standard.set(nb, forKey: Self.bookmarkKey)
    }
    invoke.resolve(["path": u.path, "stale": stale])
  }

  /// Keeps access to `u` for the app's life (Rust reads the files by path), dropping the previous one.
  private func hold(_ u: URL) {
    if let h = held, h != u { h.stopAccessingSecurityScopedResource() }
    held = u
  }
}

@_cdecl("init_plugin_capture_link")
func initPlugin() -> Plugin {
  return CaptureLinkPlugin()
}
