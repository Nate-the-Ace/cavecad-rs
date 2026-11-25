import AppKit
import Foundation

/// What `x1flash info` said about the X1 on the cable.
struct Device: Equatable {
    enum Mode: String { case app, boot, none }
    var port: String
    var mode: Mode
    var version: String?
    var bootloader: Int?
    var serial: Int?
}

/// What `x1flash inspect` said about a firmware file.
struct Firmware: Equatable {
    var path: String
    var version: String
    var built: Date
    var bytes: Int
    var packets: Int
    var warnings: [String]
    var name: String { (path as NSString).lastPathComponent }
}

enum FlashState: Equatable {
    case idle
    case running(simulated: Bool)
    case succeeded(simulated: Bool)
    case failed(message: String, appUntouched: Bool)
}

/// Runs the bundled `x1flash` and keeps the window's state.
@MainActor
final class Model: ObservableObject {
    @Published var ports: [String] = []
    @Published var port: String?
    @Published var device: Device?
    @Published var checking = false
    @Published var firmware: Firmware?
    @Published var firmwareError: String?
    @Published var flash: FlashState = .idle
    @Published var done = 0
    @Published var total = 0
    @Published var phase = ""
    @Published var log: [String] = []
    /// Talk to the X1 over Bluetooth instead of the cable (checked only when asked: a scan takes a few seconds).
    @Published var useBluetooth = UserDefaults.standard.bool(forKey: "useBluetooth") {
        didSet {
            UserDefaults.standard.set(useBluetooth, forKey: "useBluetooth")
            device = nil
            if useBluetooth { port = nil } else { refresh() }
        }
    }
    /// The feature list inside the chosen firmware file (nil: the file has none).
    @Published var fileCallouts: [String]?
    /// Set once a list has been built from the Feature callouts page, so the Flash page can say so.
    @Published var builtFromList = false

    private var timer: Timer?
    private var busy = false
    /// Set after a real flash: the X1's clock is set the first time it comes back up running.
    private var clockAfterRestart = false

    /// Cavway's published releases, where they were downloaded (newest first).
    static let library: [(String, String)] = [
        ("1.1.7-cc7m", "~/Documents/github/cavway-x1-emulator/firmware-lab/out/CVWY_X1_1.1.7-cc7m.bin"),
        ("1.1.7-cc7", "~/Documents/github/cavecad-rs-ble/tools/x1-patch/build/CVWY_X1_1.1.7-cc7.bin"),
        ("1.1.7-cc4", "~/Documents/github/cavecad-rs-ble/tools/x1-patch/build/CVWY_X1_1.1.7-cc4.bin"),
        ("1.1.7", "~/Documents/github/cavway-firmware/CVWY_X1_1.1.7.bin"),
        ("1.1.6", "~/Documents/github/cavway-firmware/CVWY_X1_1.1.6.bin"),
        ("1.1.4", "~/Documents/github/cavway-upstream/CavwayAssist/Resources/CVWY_X1_Firm_1_1_4.bin"),
    ].map { ($0.0, NSString(string: $0.1).expandingTildeInPath) }.filter { FileManager.default.fileExists(atPath: $0.1) }

    var isFlashing: Bool { if case .running = flash { return true } else { return false } }

    var canFlash: Bool { device?.mode == .boot && firmware != nil && !isFlashing }

    /// The `x1flash` helper: inside the app, else the repo's release build.
    static var helper: String {
        if let u = Bundle.main.url(forAuxiliaryExecutable: "x1flash"), FileManager.default.isExecutableFile(atPath: u.path) {
            return u.path
        }
        return NSString(string: "~/Documents/github/cavecad-rs-ble/tools/cave-x1/target/release/x1flash").expandingTildeInPath
    }

    /// The recovery card, inside the app or beside the tools.
    static var recoveryCard: URL? {
        if let u = Bundle.main.url(forResource: "recovery", withExtension: "html") { return u }
        let p = NSString(string: "~/Documents/github/cavecad-rs-ble/tools/cave-x1/recovery.html").expandingTildeInPath
        return FileManager.default.fileExists(atPath: p) ? URL(fileURLWithPath: p) : nil
    }

    func start() {
        if let last = UserDefaults.standard.string(forKey: "firmwarePath"), FileManager.default.fileExists(atPath: last) {
            choose(path: last)
        }
        refresh()
        timer = Timer.scheduledTimer(withTimeInterval: 3, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.refresh() }
        }
    }

    func note(_ s: String) {
        let t = DateFormatter.localizedString(from: Date(), dateStyle: .none, timeStyle: .medium)
        log.append("\(t)  \(s)")
        if log.count > 400 { log.removeFirst(log.count - 400) }
    }

    /// Lists ports, and checks the X1 on the chosen one (a read that changes nothing).
    func refresh(manual: Bool = false) {
        guard !busy, !isFlashing else { return }
        if useBluetooth {
            guard manual else { return }
            busy = true
            Task {
                defer { busy = false }
                checking = true
                var seen: Device?
                var problem: String?
                _ = await run(["--ble", "info"]) { ev in
                    switch ev["event"] as? String {
                    case "device":
                        seen = Device(
                            port: ev["port"] as? String ?? "Bluetooth",
                            mode: Device.Mode(rawValue: ev["mode"] as? String ?? "none") ?? .none,
                            version: ev["version"] as? String,
                            bootloader: ev["bootloader"] as? Int,
                            serial: ev["serial"] as? Int)
                    case "error": problem = ev["message"] as? String
                    default: break
                    }
                }
                checking = false
                if let problem { note("Bluetooth: \(problem)") }
                if let seen { port = seen.port; if seen != device { note(describe(seen)) } }
                device = seen
            }
            return
        }
        busy = true
        Task {
            defer { busy = false }
            var found: [String] = []
            _ = await run(["ports"]) { ev in if let p = ev["ports"] as? [String] { found = p } }
            ports = found
            if port == nil || !found.contains(port!) { port = found.first }
            guard let port else {
                if device != nil { note("X1 unplugged") }
                device = nil
                return
            }
            checking = true
            var seen: Device?
            _ = await run(["info", "--port", port]) { ev in
                guard ev["event"] as? String == "device" else { return }
                seen = Device(
                    port: port,
                    mode: Device.Mode(rawValue: ev["mode"] as? String ?? "none") ?? .none,
                    version: ev["version"] as? String,
                    bootloader: ev["bootloader"] as? Int,
                    serial: ev["serial"] as? Int)
            }
            checking = false
            if seen != device, let seen { note(describe(seen)) }
            device = seen
            if clockAfterRestart, seen?.mode == .app {
                clockAfterRestart = false
                await setClockNow()
            }
        }
    }

    func describe(_ d: Device) -> String {
        switch d.mode {
        case .app: return "X1 \(d.serial.map { "serial \($0)" } ?? ""), firmware \(d.version ?? "?") running"
        case .boot: return "X1 in boot mode (bootloader 0x\(String(format: "%02X", d.bootloader ?? 0)))"
        case .none: return "USB port found, X1 not answering"
        }
    }

    func choose(path: String) {
        firmwareError = nil
        Task {
            var fw: Firmware?
            var err: String?
            _ = await run(["inspect", path]) { ev in
                switch ev["event"] as? String {
                case "image":
                    fw = Firmware(
                        path: path,
                        version: ev["version"] as? String ?? "?",
                        built: Date(timeIntervalSince1970: TimeInterval(ev["built"] as? Int ?? 0)),
                        bytes: ev["bytes"] as? Int ?? 0,
                        packets: ev["packets"] as? Int ?? 0,
                        warnings: ev["warnings"] as? [String] ?? [])
                case "error": err = ev["message"] as? String
                default: break
                }
            }
            firmware = fw
            firmwareError = err
            fileCallouts = fw == nil ? nil : await callouts(in: path)
            if let fw {
                UserDefaults.standard.set(path, forKey: "firmwarePath")
                note("Firmware file: \(fw.name), version \(fw.version)")
            } else if let err {
                note("Not usable: \(err)")
            }
        }
    }

    func startFlash(simulated: Bool) {
        guard let fw = firmware else { return }
        if !simulated { guard canFlash else { return }; if !useBluetooth { guard port != nil else { return } } }
        flash = .running(simulated: simulated)
        done = 0
        total = fw.packets
        phase = "Starting…"
        note(simulated ? "Rehearsal (pretend X1) of \(fw.name)" : "FLASHING \(fw.name) (\(fw.version)) to \(port ?? "?")")
        Task {
            var args = ["flash", fw.path]
            if simulated { args.append("--simulate") }
            else if useBluetooth { args += ["--yes", "--ble"] }
            else { args += ["--yes", "--port", port ?? ""] }
            var outcome: FlashState = .failed(message: "x1flash stopped without saying why", appUntouched: true)
            _ = await run(args) { ev in
                switch ev["event"] as? String {
                case "phase":
                    switch ev["phase"] as? String {
                    case "erasing": self.phase = "Preparing the X1…"
                    case "installing": self.phase = "Installing on the X1 (UPGRADING...). Don't unplug."
                    case "simulating": self.phase = "Rehearsal: nothing is sent to the X1."
                    default: break
                    }
                case "progress":
                    self.done = ev["done"] as? Int ?? self.done
                    self.total = ev["total"] as? Int ?? self.total
                    self.phase = "Sending firmware: \(self.done) of \(self.total) packets"
                case "done":
                    outcome = .succeeded(simulated: simulated)
                case "failed":
                    outcome = .failed(message: ev["message"] as? String ?? "failed", appUntouched: ev["app_untouched"] as? Bool ?? true)
                case "error":
                    outcome = .failed(message: ev["message"] as? String ?? "failed", appUntouched: true)
                default: break
                }
            }
            flash = outcome
            if case .succeeded(false) = outcome { clockAfterRestart = !useBluetooth }
            switch outcome {
            case .succeeded: note(simulated ? "Rehearsal finished cleanly" : "UPGRADE SUCCESS (the clock is set when the X1 restarts)")
            case .failed(let m, _): note("Stopped: \(m)")
            default: break
            }
            if useBluetooth { device = nil }
            refresh()
        }
    }

    /// The feature list inside a firmware file, or nil if it has none (an older build).
    func callouts(in path: String) async -> [String]? {
        var names: [String]?
        _ = await run(["callouts", "show", path]) { ev in
            if ev["event"] as? String == "callouts" { names = ev["names"] as? [String] }
        }
        return names
    }

    /// Where lists built from the Feature callouts page are written.
    static var buildFolder: URL {
        let u = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("X1 Flasher")
        try? FileManager.default.createDirectory(at: u, withIntermediateDirectories: true)
        return u
    }

    /// Writes a copy of `base` with `names` as its feature list and makes it the firmware to flash. Returns an error text, or nil.
    func buildList(base: String, names: [String]) async -> String? {
        let out = Self.buildFolder.appendingPathComponent(((base as NSString).lastPathComponent as NSString).deletingPathExtension + "-list.bin").path
        var error: String?
        let code = await run(["callouts", "set", base, "--out", out, "--names", names.joined(separator: "\n")]) { ev in
            if ev["event"] as? String == "error" { error = ev["message"] as? String }
        }
        if let error { return error }
        if code != 0 { return "x1flash stopped (exit \(code))" }
        choose(path: out)
        builtFromList = true
        note("Built \((out as NSString).lastPathComponent) with \(names.count) feature names")
        return nil
    }

    /// Sets the X1's clock to this Mac's local time.
    func setClock() {
        Task { await setClockNow() }
    }

    private func setClockNow() async {
        guard let port else { return }
        var said = "Clock: no answer"
        _ = await run(["clock"] + (useBluetooth ? ["--ble"] : ["--port", port])) { ev in
            if ev["event"] as? String == "clock" {
                said = (ev["confirmed"] as? Bool ?? false) ? "Clock set to this Mac's time (confirmed)" : "Clock set to this Mac's time (sent)"
            } else if ev["event"] as? String == "error" {
                said = "Clock: \(ev["message"] as? String ?? "failed")"
            }
        }
        note(said)
    }

    func powerOff() {
        guard let port else { return }
        note("Power off sent")
        Task { _ = await run(["off"] + (useBluetooth ? ["--ble"] : ["--port", port])); device = nil; refresh() }
    }

    /// Runs `x1flash --json …`, handing each JSON line to `onEvent` on the main actor. Returns the exit code.
    @discardableResult
    func run(_ args: [String], onEvent: @escaping ([String: Any]) -> Void = { _ in }) async -> Int32 {
        let helper = Self.helper
        guard FileManager.default.isExecutableFile(atPath: helper) else {
            note("x1flash is missing (\(helper)); rebuild the app with build-app.sh")
            return -1
        }
        return await withCheckedContinuation { cont in
            let p = Process()
            p.executableURL = URL(fileURLWithPath: helper)
            p.arguments = ["--json"] + args
            let pipe = Pipe()
            p.standardOutput = pipe
            p.standardError = Pipe()
            do { try p.run() } catch {
                cont.resume(returning: -1)
                return
            }
            // one reader thread: lines go to the main actor in order, then the exit code
            DispatchQueue.global(qos: .userInitiated).async {
                let h = pipe.fileHandleForReading
                var pending = Data()
                func deliver(_ line: Data) {
                    if let obj = try? JSONSerialization.jsonObject(with: line) as? [String: Any] {
                        DispatchQueue.main.async { onEvent(obj) }
                    }
                }
                while true {
                    let chunk = h.availableData
                    if chunk.isEmpty { break }
                    pending.append(chunk)
                    while let nl = pending.firstIndex(of: 0x0a) {
                        deliver(Data(pending[pending.startIndex..<nl]))
                        pending.removeSubrange(pending.startIndex...nl)
                    }
                }
                if !pending.isEmpty { deliver(pending) }
                p.waitUntilExit()
                let code = p.terminationStatus
                DispatchQueue.main.async { cont.resume(returning: code) }
            }
        }
    }
}
