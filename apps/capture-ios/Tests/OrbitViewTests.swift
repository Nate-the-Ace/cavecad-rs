import XCTest
@testable import CaveCAD_Capture

final class OrbitViewTests: XCTestCase {
    func near(_ a: SIMD3<Float>, _ b: SIMD3<Float>, _ tol: Float = 1e-4, file: StaticString = #filePath, line: UInt = #line) {
        XCTAssertLessThan(simd_length(a - b), tol, "\(a) vs \(b)", file: file, line: line)
    }

    func testCameraPosition() {
        let t = SIMD3<Float>(1, 2, 3)
        near(OrbitRig.cameraPosition(target: t, yaw: 0, pitch: 0, distance: 5), t + SIMD3(0, 0, 5))
        near(OrbitRig.cameraPosition(target: t, yaw: .pi / 2, pitch: 0, distance: 5), t + SIMD3(5, 0, 0))
        // looking down: the camera is above the target
        near(OrbitRig.cameraPosition(target: .zero, yaw: 0, pitch: -.pi / 2, distance: 2), SIMD3(0, 2, 0))
        // always `distance` away
        XCTAssertEqual(simd_length(OrbitRig.cameraPosition(target: .zero, yaw: 0.7, pitch: -0.4, distance: 3)), 3, accuracy: 1e-4)
    }

    /// The camera's axes agree with a node turned (pitch, yaw, 0) and are square to the view direction.
    func testCameraAxes() {
        let yaw: Float = 0.8, pitch: Float = -0.5
        let back = OrbitRig.back(yaw: yaw, pitch: pitch), r = OrbitRig.right(yaw: yaw), u = OrbitRig.up(yaw: yaw, pitch: pitch)
        XCTAssertEqual(simd_dot(back, r), 0, accuracy: 1e-5)
        XCTAssertEqual(simd_dot(back, u), 0, accuracy: 1e-5)
        XCTAssertEqual(simd_dot(r, u), 0, accuracy: 1e-5)
        near(simd_cross(r, u), back)  // right-handed: right × up = backwards
        let n = SCNNodeAxes(yaw: yaw, pitch: pitch)
        near(n.right, r); near(n.up, u); near(n.back, back)
    }

    func testPanShift() {
        let d: Float = 10, fov = OrbitRig.fov, h: Float = 800
        let visibleHeight = 2 * d * tan(fov / 2)
        // fingers across the whole height downward: the target goes up by the visible height (scene follows)
        near(OrbitRig.panShift(dx: 0, dy: h, yaw: 0, pitch: 0, distance: d, fov: fov, viewHeight: h), SIMD3(0, visibleHeight, 0))
        // fingers right by the view's width: the target goes left by the visible width
        let w: Float = 400
        near(OrbitRig.panShift(dx: w, dy: 0, yaw: 0, pitch: 0, distance: d, fov: fov, viewHeight: h),
             SIMD3(-visibleHeight * w / h, 0, 0))
        // twice as far, twice the shift
        let a = OrbitRig.panShift(dx: 3, dy: -2, yaw: 1, pitch: -0.3, distance: 5, fov: fov, viewHeight: h)
        let b = OrbitRig.panShift(dx: 3, dy: -2, yaw: 1, pitch: -0.3, distance: 10, fov: fov, viewHeight: h)
        near(2 * a, b)
        XCTAssertEqual(OrbitRig.panShift(dx: 3, dy: 3, yaw: 0, pitch: 0, distance: 5, fov: fov, viewHeight: 0), .zero)
    }

    func testPitchClamp() {
        XCTAssertEqual(OrbitRig.clampPitch(2), OrbitRig.maxPitch)
        XCTAssertEqual(OrbitRig.clampPitch(-2), -OrbitRig.maxPitch)
        XCTAssertEqual(OrbitRig.clampPitch(0.3), 0.3)
        XCTAssertEqual(OrbitRig.maxPitch, 89 * .pi / 180, accuracy: 1e-6)
    }

    func testFraming() {
        let lo = SIMD3<Float>(-30, -2, -10), hi = SIMD3<Float>(30, 4, 10)
        let r = simd_length(hi - lo) / 2
        let d = OrbitRig.framingDistance(min: lo, max: hi, fov: OrbitRig.fov)
        // the bounding sphere fits: r / d is inside sin(fov/2)
        XCTAssertLessThan(r / d, sin(OrbitRig.fov / 2))
        XCTAssertGreaterThan(d, r)
        let rig = OrbitRig.framing(min: lo, max: hi)
        near(rig.target, SIMD3(0, 1, 0))
        XCTAssertEqual(rig.pitch, -30 * .pi / 180, accuracy: 1e-6)
        XCTAssertEqual(rig.distance, d)
        XCTAssertTrue(OrbitRig.distanceRange(radius: r).contains(d))
        // an empty box still frames
        XCTAssertGreaterThan(OrbitRig.framingDistance(min: .zero, max: .zero, fov: OrbitRig.fov), 0)
    }
}

import SceneKit
/// What SceneKit itself makes of a node turned (pitch, yaw, 0).
private func SCNNodeAxes(yaw: Float, pitch: Float) -> (right: SIMD3<Float>, up: SIMD3<Float>, back: SIMD3<Float>) {
    let n = SCNNode()
    n.simdEulerAngles = SIMD3(pitch, yaw, 0)
    let m = n.simdTransform
    return (SIMD3(m.columns.0.x, m.columns.0.y, m.columns.0.z), SIMD3(m.columns.1.x, m.columns.1.y, m.columns.1.z),
            SIMD3(m.columns.2.x, m.columns.2.y, m.columns.2.z))
}
