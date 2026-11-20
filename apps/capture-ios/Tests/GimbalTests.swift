import XCTest
@testable import CaveCAD_Capture

final class GimbalTests: XCTestCase {
    /// The core's gimbal runner starts idle and refuses a bad plan without touching Bluetooth.
    func testStatusStartsIdleAndBadPlanRefused() {
        let s = Core.gimbalStatus()
        XCTAssertEqual(s["state"] as? String, "idle")
        XCTAssertEqual(s["connected"] as? Bool, false)
        XCTAssertEqual(s["mount_ok"] as? Bool, false)
        XCTAssertNotNil(Core.gimbalRun([]))

        // no gimbal here (the simulator has no Bluetooth): a connect ends in a readable error, not a hang
        Core.gimbalConnect(timeoutMs: 1000)
        let end = Date().addingTimeInterval(30)
        var last: [String: Any] = [:]
        while Date() < end {
            last = Core.gimbalStatus()
            if let st = last["state"] as? String, st != "connecting", st != "idle" { break }
            Thread.sleep(forTimeInterval: 0.1)
        }
        XCTAssertEqual(last["state"] as? String, "error", "\(last)")
        XCTAssertFalse((last["error"] as? String ?? "").isEmpty)
        Core.gimbalDisconnect()
    }
}
