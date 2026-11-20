import Foundation
import os
import UIKit

/// The app's own log, for testing and bug catching: Documents/capture.log on the phone, read from the
/// Mac with `apps/capture-ios/pull-log.sh` (also mirrored to the system log, subsystem app.cavecad.capture).
/// Never write tokens, codes, cave contents or coordinates here: only what happened and why.
enum Log {
    nonisolated static let file: URL = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        .appendingPathComponent("capture.log")
    /// The log is cut back to its newest half past this size.
    nonisolated static let maxBytes = 2_000_000
    private nonisolated static let queue = DispatchQueue(label: "app.cavecad.capture.log")
    private nonisolated static let system = Logger(subsystem: "app.cavecad.capture", category: "app")

    nonisolated static func write(_ area: String, _ text: String) {
        let line = "\(ISO8601DateFormatter().string(from: Date())) [\(area)] \(text)\n"
        system.log("[\(area, privacy: .public)] \(text, privacy: .public)")
        queue.async {
            let fm = FileManager.default
            if !fm.fileExists(atPath: file.path) { fm.createFile(atPath: file.path, contents: nil) }
            if let size = (try? fm.attributesOfItem(atPath: file.path)[.size] as? Int), size > maxBytes,
               let data = try? Data(contentsOf: file) {
                try? data.suffix(maxBytes / 2).write(to: file, options: .atomic)
            }
            guard let h = try? FileHandle(forWritingTo: file) else { return }
            defer { try? h.close() }
            _ = try? h.seekToEnd()
            try? h.write(contentsOf: Data(line.utf8))
        }
    }

    /// One line at launch: app build, phone, iOS.
    @MainActor static func launched() {
        let info = Bundle.main.infoDictionary ?? [:]
        var s = utsname(); uname(&s)
        let model = withUnsafeBytes(of: s.machine) { b in String(decoding: b.prefix { $0 != 0 }, as: UTF8.self) }
        write("app", "launch \(info["CFBundleShortVersionString"] ?? "?") (\(info["CFBundleVersion"] ?? "?")) on \(model) iOS \(UIDevice.current.systemVersion)")
        NSSetUncaughtExceptionHandler { e in Log.write("crash", "uncaught \(e.name.rawValue): \(e.reason ?? "")") }
    }
}
