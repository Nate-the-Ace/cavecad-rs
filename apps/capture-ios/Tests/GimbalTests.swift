import XCTest
@testable import CaveCAD_Capture

final class FakeGimbal: GimbalDriver {
    var st = GimbalStatus(connected: true, state: "idle", mountOK: true)
    var runs: [[[[String: Double]]]] = [], holds = 0, goOns = 0, stops = 0
    func connect() {}
    func run(_ l: [[[String: Double]]]) -> String? { runs.append(l); st.state = "moving"; return nil }
    func hold() { holds += 1 }
    func goOn() { goOns += 1 }
    func stopHome() { stops += 1; st.state = "homing" }
    func status() -> GimbalStatus { st }
}

@MainActor
final class GimbalTests: XCTestCase {
    func inputs(t: Double, tracking: GimbalRun.Tracking = .normal, lux: Double = 1000, battery: Double = 0.9,
                missing: [(Double, Double)] = []) -> GimbalRun.Inputs {
        GimbalRun.Inputs(time: t, tracking: tracking, lux: lux, battery: battery, missing: missing)
    }

    func testCameraPlans() {
        let sweep = Core.cameraSweep(speed: 10)
        XCTAssertFalse(sweep.isEmpty)
        XCTAssertEqual(sweep.last?.last?["yaw"], 0)
        XCTAssertTrue(Core.cameraSweep(speed: 0).isEmpty)
        XCTAssertFalse(Core.cameraFill(missing: [(10, 5)], speed: 10).isEmpty)
        XCTAssertTrue(Core.cameraFill(missing: [(0, 85)], speed: 10).isEmpty)
    }

    func testChecksRefuseWithReasons() {
        let g = FakeGimbal()
        XCTAssertNil(GimbalRun.check(g.status(), tracking: .normal, lux: 1000))
        g.st.connected = false
        XCTAssertEqual(GimbalRun.check(g.status(), tracking: .normal, lux: 1000), "Gimbal not connected.")
        g.st.connected = true; g.st.mountOK = false
        XCTAssertEqual(GimbalRun.check(g.status(), tracking: .normal, lux: 1000), "Put the phone upright (portrait) in the gimbal.")
        g.st.mountOK = true
        XCTAssertEqual(GimbalRun.check(g.status(), tracking: .limited, lux: 1000), "Hold still until tracking settles.")
        XCTAssertEqual(GimbalRun.check(g.status(), tracking: .normal, lux: 100), "Too dark: check the lights.")
    }

    func testSweepThenFillThenDone() {
        let g = FakeGimbal(), r = GimbalRun(gimbal: g)
        XCTAssertNil(r.start(at: 0))
        XCTAssertEqual(g.runs.count, 1)                  // sweep
        XCTAssertEqual(r.stage, .sweep)
        XCTAssertEqual(r.planned, g.runs[0].reduce(0) { $0 + $1.count })
        g.st.state = "done"
        _ = r.tick(inputs(t: 60, missing: [(10, 5)]))    // sweep done: fill planned and started
        XCTAssertEqual(g.runs.count, 2)
        XCTAssertEqual(r.stage, .fill)
        XCTAssertEqual(r.planned, g.runs[1].reduce(0) { $0 + $1.count })
        g.st.state = "done"
        XCTAssertEqual(r.tick(inputs(t: 90)), .finished(flags: []))
    }

    func testNoReachableHolesSkipsFill() {
        let g = FakeGimbal(), r = GimbalRun(gimbal: g)
        _ = r.start(at: 0); g.st.state = "done"
        XCTAssertEqual(r.tick(inputs(t: 60, missing: [(0, 85)])), .finished(flags: []))
        XCTAssertEqual(g.runs.count, 1)
    }

    func testFillCappedAt120Seconds() {
        let g = FakeGimbal(), r = GimbalRun(gimbal: g)
        _ = r.start(at: 0); g.st.state = "done"
        _ = r.tick(inputs(t: 60, missing: [(10, 5)]))
        XCTAssertEqual(r.tick(inputs(t: 150)), .running)
        XCTAssertEqual(g.stops, 0)
        XCTAssertEqual(r.tick(inputs(t: 181)), .running)
        XCTAssertEqual(g.stops, 1)
        g.st.state = "done"
        XCTAssertEqual(r.tick(inputs(t: 190)), .finished(flags: []))
    }

    func testTrackingHoldsAndLostEnds() {
        let g = FakeGimbal(), r = GimbalRun(gimbal: g)
        _ = r.start(at: 0)
        _ = r.tick(inputs(t: 1, tracking: .limited)); XCTAssertEqual(g.holds, 1)
        XCTAssertTrue(r.holding); XCTAssertEqual(r.holdReason, "tracking")
        _ = r.tick(inputs(t: 2, tracking: .normal)); XCTAssertEqual(g.goOns, 1)
        XCTAssertFalse(r.holding); XCTAssertNil(r.holdReason)
        _ = r.tick(inputs(t: 3, tracking: .lost))
        XCTAssertEqual(r.tick(inputs(t: 9, tracking: .lost)), .running)
        _ = r.tick(inputs(t: 14, tracking: .lost))
        XCTAssertEqual(g.stops, 1)
        XCTAssertEqual(r.tick(inputs(t: 15, tracking: .lost)), .running)   // not done yet
        XCTAssertEqual(g.stops, 1)
        g.st.state = "done"
        XCTAssertEqual(r.tick(inputs(t: 20, tracking: .lost)), .finished(flags: ["tracking lost"]))
    }

    func testDarkHoldsAfterThreeSeconds() {
        let g = FakeGimbal(), r = GimbalRun(gimbal: g)
        _ = r.start(at: 0)
        _ = r.tick(inputs(t: 1, lux: 50)); XCTAssertEqual(g.holds, 0)
        _ = r.tick(inputs(t: 4.5, lux: 50)); XCTAssertEqual(g.holds, 1)
        XCTAssertEqual(r.message, "Too dark: check the lights")
        _ = r.tick(inputs(t: 5, lux: 900)); XCTAssertEqual(g.goOns, 1)
    }

    func testGimbalLostAndLowBattery() {
        var g = FakeGimbal(), r = GimbalRun(gimbal: g)
        _ = r.start(at: 0); g.st.connected = false
        XCTAssertEqual(r.tick(inputs(t: 1)), .finished(flags: ["gimbal lost"]))
        g = FakeGimbal(); r = GimbalRun(gimbal: g)
        _ = r.start(at: 0)
        _ = r.tick(inputs(t: 1, battery: 0.1)); XCTAssertEqual(g.stops, 1)
        g.st.state = "done"
        XCTAssertEqual(r.tick(inputs(t: 2, battery: 0.1)), .finished(flags: ["low battery"]))
        g = FakeGimbal(); r = GimbalRun(gimbal: g)
        _ = r.start(at: 0); g.st.state = "error"
        XCTAssertEqual(r.tick(inputs(t: 1)), .finished(flags: ["gimbal lost"]))
    }

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
