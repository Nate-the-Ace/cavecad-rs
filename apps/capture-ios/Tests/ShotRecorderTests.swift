import XCTest
import simd
@testable import CaveCAD_Capture

final class ShotRecorderTests: XCTestCase {
    func testKeyframeRule() {
        XCTAssertFalse(ShotRecorder.shouldKeep(lastYaw: 0, yaw: 10, lastTime: 0, time: 1))
        XCTAssertTrue(ShotRecorder.shouldKeep(lastYaw: 0, yaw: 16, lastTime: 0, time: 1))
        XCTAssertTrue(ShotRecorder.shouldKeep(lastYaw: 0, yaw: 1, lastTime: 0, time: 2.1))
        XCTAssertFalse(ShotRecorder.shouldKeep(lastYaw: 0, yaw: 90, lastTime: 0, time: 0.1))
        XCTAssertTrue(ShotRecorder.shouldKeep(lastYaw: 350, yaw: 6, lastTime: 0, time: 1)) // wraps
    }

    func testMergeAnchors() {
        let tri = MeshPart(positions: [SIMD3(0, 0, 0), SIMD3(1, 0, 0), SIMD3(0, 1, 0)],
                           normals: Array(repeating: SIMD3(0, 0, 1), count: 3), indices: [0, 1, 2],
                           transform: matrix_identity_float4x4)
        var moved = tri
        moved.transform.columns.3 = SIMD4(10, 0, 0, 1)
        let m = ShotRecorder.mergeAnchors([tri, moved])
        XCTAssertEqual(m.positions.count, 18)
        XCTAssertEqual(m.indices, [0, 1, 2, 3, 4, 5])
        XCTAssertEqual(m.positions[9], 10)
    }

    /// The Stop path without ARKit: merge, header, Rust encode, file write, then the summary places it.
    func testShotJobWritesAPlacedBundle() throws {
        let tri = MeshPart(positions: [SIMD3(0, 0, -1), SIMD3(1, 0, -1), SIMD3(0, 1, -1)],
                           normals: Array(repeating: SIMD3(0, 0, 1), count: 3), indices: [0, 1, 2],
                           transform: matrix_identity_float4x4)
        let over = Station(id: "a", label: "A1", x: 0, y: 0, z: 0)
        let forward = Station(id: "b", label: "A2", x: 0, y: 10, z: 0)
        let kf = ShotKeyframe(pose: matrix_identity_float4x4, intrinsics: matrix_identity_float3x3, w: 1, h: 1, jpeg: Data([1, 2, 3]))
        let job = ShotJob(caveId: "test-cave-shotjob", over: over, forward: forward, parts: [tri], keyframes: [kf],
                          tapOrigin: .zero, tapDir: SIMD3(0, 0, -1), startCamera: .zero, tracking: "normal", systemVersion: "26.0")
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: "test-cave-shotjob")) }
        guard case .success(let url) = job.write() else { return XCTFail("shot not written") }
        XCTAssertEqual(url.pathExtension, "cvcap")
        let s = Core.summary(bundle: try Data(contentsOf: url), pair: Core.pair(over, forward))
        XCTAssertEqual(s["placed"] as? Bool, true, "\(s)")
        XCTAssertEqual(s["vertices"] as? Int, 3)
        XCTAssertEqual(s["triangles"] as? Int, 1)
        XCTAssertEqual(s["keyframes"] as? Int, 1)
        XCTAssertEqual(ShotFiles.list(cave: "test-cave-shotjob"), [url])
    }

    func testRaggedMeshIsRefused() {
        let bad = MeshPart(positions: [SIMD3(0, 0, 0), SIMD3(1, 0, 0), SIMD3(0, 1, 0)],
                           normals: [SIMD3(0, 0, 1)], indices: [0, 1, 2], transform: matrix_identity_float4x4)
        let st = Station(id: "a", label: "A1", x: 0, y: 0, z: 0)
        let job = ShotJob(caveId: "test-cave-ragged", over: st, forward: st, parts: [bad], keyframes: [],
                          tapOrigin: .zero, tapDir: SIMD3(0, 0, -1), startCamera: .zero, tracking: "normal", systemVersion: "26.0")
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: "test-cave-ragged")) }
        guard case .failure(let f) = job.write() else { return XCTFail("ragged mesh written") }
        XCTAssertEqual(f.message, "The shot could not be written.")
    }
}
