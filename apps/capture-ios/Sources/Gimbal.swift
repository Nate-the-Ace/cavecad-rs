import Foundation

/// The gimbal as the capture app sees it (the real one in the Rust core, or a fake in tests).
protocol GimbalDriver: AnyObject {
    func connect()
    /// nil when started, else why not.
    func run(_ lists: [[[String: Double]]]) -> String?
    func hold()
    func goOn()
    func stopHome()
    func status() -> GimbalStatus
}

/// What the gimbal reports (`cc_gimbal_status`).
struct GimbalStatus: Equatable, Sendable {
    var connected = false
    /// idle, connecting, moving, holding, homing, done, error
    var state = "idle"
    var step = 0, steps = 0
    var mountOK = false
    var error: String?

    init(connected: Bool = false, state: String = "idle", step: Int = 0, steps: Int = 0, mountOK: Bool = false, error: String? = nil) {
        self.connected = connected; self.state = state; self.step = step; self.steps = steps; self.mountOK = mountOK; self.error = error
    }

    init(_ d: [String: Any]) {
        connected = d["connected"] as? Bool ?? false
        state = d["state"] as? String ?? "idle"
        step = (d["step"] as? NSNumber)?.intValue ?? 0
        steps = (d["steps"] as? NSNumber)?.intValue ?? 0
        mountOK = d["mount_ok"] as? Bool ?? false
        error = d["error"] as? String
    }
}

/// The Osmo through the Rust core's runner.
final class CoreGimbal: GimbalDriver {
    func connect() { Core.gimbalConnect() }
    func run(_ lists: [[[String: Double]]]) -> String? { Core.gimbalRun(lists) }
    func hold() { Core.gimbalHold() }
    func goOn() { Core.gimbalGoOn() }
    func stopHome() { Core.gimbalStopHome() }
    func status() -> GimbalStatus { GimbalStatus(Core.gimbalStatus()) }
}
