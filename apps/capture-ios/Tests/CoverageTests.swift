import XCTest
import simd
@testable import CaveCAD_Capture

final class CoverageTests: XCTestCase {
    func box(without missingWall: Int? = nil) -> MeshSnapshot {
        // walls at x=±2, z=±2, floor y=-1.5, ceiling y=2
        let quads: [[SIMD3<Float>]] = [
            [SIMD3(-2,-1.5,-2), SIMD3(2,-1.5,-2), SIMD3(2,2,-2), SIMD3(-2,2,-2)],   // 0: front (-z)
            [SIMD3(-2,-1.5, 2), SIMD3(2,-1.5, 2), SIMD3(2,2, 2), SIMD3(-2,2, 2)],   // 1: back (+z)
            [SIMD3( 2,-1.5,-2), SIMD3(2,-1.5, 2), SIMD3(2,2, 2), SIMD3(2,2,-2)],    // 2: right (+x)
            [SIMD3(-2,-1.5,-2), SIMD3(-2,-1.5,2), SIMD3(-2,2,2), SIMD3(-2,2,-2)],   // 3: left (-x)
            [SIMD3(-2,-1.5,-2), SIMD3(2,-1.5,-2), SIMD3(2,-1.5,2), SIMD3(-2,-1.5,2)], // 4: floor
            [SIMD3(-2, 2,-2), SIMD3(2, 2,-2), SIMD3(2, 2,2), SIMD3(-2, 2,2)],       // 5: ceiling
        ]
        let parts = quads.enumerated().filter { $0.offset != missingWall }.map {
            MeshPart(positions: $0.element, normals: [], indices: [0, 1, 2, 0, 2, 3], transform: matrix_identity_float4x4)
        }
        return MeshSnapshot(parts)
    }

    func testClosedBoxHasNoHoles() {
        XCTAssertTrue(Coverage.missing(mesh: box(), pivot: .zero, homeForward: SIMD3(0, 0, -1)).isEmpty)
    }

    func testMissingWallShowsUpRelativeToHome() {
        let m = Coverage.missing(mesh: box(without: 2), pivot: .zero, homeForward: SIMD3(0, 0, -1))  // right wall (+x) gone
        XCTAssertFalse(m.isEmpty)
        XCTAssertTrue(m.allSatisfy { $0.yaw > 30 && $0.yaw < 150 }, "\(m)")   // to the right of home: +yaw with yawSign +1
        let mean = m.map(\.yaw).reduce(0, +) / Double(m.count)
        XCTAssertEqual(mean, 90, accuracy: 10)
    }

    func testYawSignFlipsAndHomeHeadingIsSubtracted() {
        Coverage.yawSign = -1
        defer { Coverage.yawSign = 1 }
        let m = Coverage.missing(mesh: box(without: 2), pivot: .zero, homeForward: SIMD3(0, 0, -1))
        XCTAssertTrue(m.allSatisfy { $0.yaw < -30 && $0.yaw > -150 }, "\(m)")
    }

    /// 165…180 is reachable the other way round (−195…−180); 80…165 stays as it is (out of reach either way).
    func testYawPastTheHighEndWrapsWhenReachable() {
        XCTAssertEqual(Coverage.reachableYaw(175), -185)
        XCTAssertEqual(Coverage.reachableYaw(165), -195)
        XCTAssertEqual(Coverage.reachableYaw(155), 155)
        XCTAssertEqual(Coverage.reachableYaw(60), 60)
        XCTAssertEqual(Coverage.reachableYaw(-175), -175)
        // the back wall (+z) gone: the cells just behind on the left come out as −185 and −195
        let m = Coverage.missing(mesh: box(without: 1), pivot: .zero, homeForward: SIMD3(0, 0, -1))
        let yaws = Set(m.map(\.yaw))
        XCTAssertTrue(yaws.contains(-185) && yaws.contains(-195), "\(yaws.sorted())")
        XCTAssertFalse(yaws.contains { $0 > 160 }, "\(yaws.sorted())")
        XCTAssertTrue(yaws.contains(155))
    }
}
