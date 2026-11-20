import Foundation

/// One gimbal pass: sweep, then fill the reachable holes, then done. Pure decisions from plain inputs, so it is
/// tested without ARKit or Bluetooth; `ShotRecorder` feeds it a few times a second.
@MainActor
final class GimbalRun {
    enum Tracking: Equatable { case normal, limited, lost }
    enum Outcome: Equatable { case running, finished(flags: [String]) }
    struct Inputs {
        var time: Double
        var tracking: Tracking
        var lux: Double
        var battery: Double
        /// Missing directions (yaw, pitch) relative to home, for the fill stage.
        var missing: [(Double, Double)]
    }

    static let minLux = 400.0
    static let darkAfter = 3.0, lostAfter = 10.0, fillMax = 120.0, lowBattery = 0.15
    /// Sweep speed, degrees per second (bench-tuned).
    static var speed = 10.0

    private enum Stage { case sweep, fill, ending }
    private let gimbal: GimbalDriver
    private var stage = Stage.sweep
    private var holding = false
    private var darkSince: Double?, lostSince: Double?, fillStart: Double?
    private var flags: [String] = []
    /// What to show the user.
    private(set) var message = ""

    init(gimbal: GimbalDriver) { self.gimbal = gimbal }

    /// Why a pass cannot start now, or nil.
    static func check(_ s: GimbalStatus, tracking: Tracking, lux: Double) -> String? {
        if !s.connected { return "Gimbal not connected." }
        if !s.mountOK { return "Put the phone upright (portrait) in the gimbal." }
        if tracking != .normal { return "Hold still until tracking settles." }
        if lux < minLux { return "Too dark: check the lights." }
        return nil
    }

    /// Starts the sweep; nil when started, else why not.
    func start(at t: Double) -> String? {
        let lists = Core.cameraSweep(speed: Self.speed)
        if let e = gimbal.run(lists) { return e }
        stage = .sweep; holding = false; message = "Sweeping…"
        return nil
    }

    func tick(_ i: Inputs) -> Outcome {
        let s = gimbal.status()
        if !s.connected || s.state == "error" { return .finished(flags: flags + ["gimbal lost"]) }
        if stage == .ending { return s.state == "done" ? .finished(flags: flags) : .running }
        // conditions that end the pass
        if i.tracking == .lost {
            let since = lostSince ?? i.time
            lostSince = since
            if i.time - since > Self.lostAfter { return end("tracking lost") }
        } else { lostSince = nil }
        if i.battery < Self.lowBattery { return end("low battery") }
        if stage == .fill, let f = fillStart, i.time - f > Self.fillMax { return end(nil) }
        // holds
        let dark = i.lux < Self.minLux
        darkSince = dark ? (darkSince ?? i.time) : nil
        let tooDark = darkSince.map { i.time - $0 > Self.darkAfter } ?? false
        let shouldHold = i.tracking != .normal || tooDark
        if shouldHold != holding {
            holding = shouldHold
            if holding { gimbal.hold() } else { gimbal.goOn() }
        }
        message = tooDark ? "Too dark: check the lights" : (i.tracking != .normal ? "Holding: tracking" : (stage == .sweep ? "Sweeping…" : "Filling holes…"))
        // stage changes
        if s.state == "done" {
            switch stage {
            case .sweep:
                let lists = Core.cameraFill(missing: i.missing, speed: Self.speed)
                if lists.isEmpty { return .finished(flags: flags) }
                if let e = gimbal.run(lists) { return .finished(flags: flags + ["gimbal lost: \(e)"]) }
                stage = .fill; fillStart = i.time
            case .fill: return .finished(flags: flags)
            case .ending: break
            }
        }
        return .running
    }

    private func end(_ flag: String?) -> Outcome {
        if let flag { flags.append(flag) }
        holding = false
        gimbal.stopHome(); stage = .ending; message = "Going home…"
        return .running
    }
}
