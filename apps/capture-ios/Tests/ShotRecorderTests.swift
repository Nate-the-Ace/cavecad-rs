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
    static let tri = MeshPart(positions: [SIMD3(0, 0, -1), SIMD3(1, 0, -1), SIMD3(0, 1, -1)],
                              normals: Array(repeating: SIMD3(0, 0, 1), count: 3), indices: [0, 1, 2],
                              transform: matrix_identity_float4x4)

    /// A compass shot: no taps, the compass reading, and the gimbal's pivot height; it places from its station alone.
    func testShotJobWritesACompassShot() throws {
        let over = Station(id: "a", label: "A1", x: 100, y: 200, z: 50)
        let kf = ShotKeyframe(pose: matrix_identity_float4x4, intrinsics: matrix_identity_float3x3, w: 1, h: 1, jpeg: Data([1, 2, 3]))
        let job = ShotJob(caveId: "test-cave-shotjob", over: over, parts: [Self.tri], keyframes: [kf], startCamera: .zero,
                          tracking: "normal", systemVersion: "26.0",
                          compass: CompassReading(trueNorth: true, accuracyDeg: 8, headingDeg: 12))
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: "test-cave-shotjob")) }
        guard case .success(let url) = job.write() else { return XCTFail("shot not written") }
        XCTAssertEqual(url.pathExtension, "cvcap")
        let header = try Self.header(of: Data(contentsOf: url))
        XCTAssertEqual(header["instrument_height_m"] as? Double, 0.30)
        XCTAssertEqual((header["taps"] as? [Any])?.count, 0)
        let compass = header["compass"] as? [String: Any]
        XCTAssertEqual(compass?["true_north"] as? Bool, true)
        XCTAssertEqual(compass?["accuracy_deg"] as? Double, 8)
        let s = ShotSummary(Core.summary(bundle: try Data(contentsOf: url), pair: Core.pair(over, over)))
        XCTAssertTrue(s.placed)
        XCTAssertTrue(s.isCompass)
        XCTAssertEqual(s.vertices, 3)
        XCTAssertEqual(s.keyframes, 1)
        XCTAssertEqual(s.stationId, "a")
        XCTAssertNil(s.targetStationId)
        // (0,0,-1) from the start camera is 1 m north, 0.30 m above the station
        let pts = Core.placedPositions(bundle: try Data(contentsOf: url), pair: Core.pair(over, over))
        XCTAssertEqual(pts[0], 0, accuracy: 1e-5); XCTAssertEqual(pts[1], 1, accuracy: 1e-5); XCTAssertEqual(pts[2], 0.30, accuracy: 1e-5)
        XCTAssertEqual(ShotFiles.list(cave: "test-cave-shotjob"), [url])
    }

    func testCompassReading() {
        let good = CompassReading(trueHeading: 91, magneticHeading: 90, accuracy: 5)
        XCTAssertEqual(good, CompassReading(trueNorth: true, accuracyDeg: 5, headingDeg: 91))
        XCTAssertNil(good.warning)
        let magnetic = CompassReading(trueHeading: -1, magneticHeading: 90, accuracy: 5)
        XCTAssertEqual(magnetic, CompassReading(trueNorth: false, accuracyDeg: 5, headingDeg: 90))
        XCTAssertEqual(CompassReading(trueHeading: 10, magneticHeading: 9, accuracy: 25).warning,
                       "Compass unsure (±25°). The gimbal's magnet may be pulling it; the shot will still save.")
        XCTAssertEqual(CompassReading(trueHeading: -1, magneticHeading: 0, accuracy: -1).warning,
                       "Compass unsure (±?°). The gimbal's magnet may be pulling it; the shot will still save.")
        XCTAssertNotNil(CompassReading.none.warning)
    }

    /// The bundle's header JSON (after magic, u16 format, u32 length).
    static func header(of b: Data) throws -> [String: Any] {
        let b = [UInt8](b)
        let at = 6 + 2
        let len = Int(b[at]) | Int(b[at + 1]) << 8 | Int(b[at + 2]) << 16 | Int(b[at + 3]) << 24
        let wire = try JSONSerialization.jsonObject(with: Data(b[(at + 4)..<(at + 4 + len)])) as? [String: Any]
        return wire?["header"] as? [String: Any] ?? [:]
    }

    func testNonFiniteHeaderIsRefused() {
        let st = Station(id: "a", label: "A1", x: 0, y: 0, z: 0)
        let tri = MeshPart(positions: [SIMD3(0, 0, -1), SIMD3(1, 0, -1), SIMD3(0, 1, -1)],
                           normals: Array(repeating: SIMD3(0, 0, 1), count: 3), indices: [0, 1, 2],
                           transform: matrix_identity_float4x4)
        let job = ShotJob(caveId: "test-cave-nan", over: st, parts: [tri], keyframes: [], startCamera: SIMD3(.nan, 0, 0),
                          tracking: "normal", systemVersion: "26.0", compass: .none)
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: "test-cave-nan")) }
        guard case .failure(let f) = job.write() else { return XCTFail("NaN header written") }
        XCTAssertEqual(f.message, "The shot could not be written.")
    }

    func testHasMesh() {
        XCTAssertFalse(ShotRecorder.hasMesh([]))
        XCTAssertFalse(ShotRecorder.hasMesh([MeshPart(positions: [], normals: [], indices: [], transform: matrix_identity_float4x4)]))
        XCTAssertTrue(ShotRecorder.hasMesh([MeshPart(positions: [.zero], normals: [.zero], indices: [], transform: matrix_identity_float4x4)]))
    }

    func testPivotHeightMirrorsTheCore() {
        XCTAssertEqual(Rig.pivotHeightM, 0.30)  // cave_capture::frame::PIVOT_HEIGHT_M
    }

    func testShotListNewestFirst() throws {
        let cave = "test-cave-order"
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: cave)) }
        let older = try ShotFiles.save(Data([1]), cave: cave, shot: "zzz-older")
        let newer = try ShotFiles.save(Data([2]), cave: cave, shot: "aaa-newer")
        try FileManager.default.setAttributes([.creationDate: Date(timeIntervalSince1970: 1000)], ofItemAtPath: older.path)
        try FileManager.default.setAttributes([.creationDate: Date(timeIntervalSince1970: 2000)], ofItemAtPath: newer.path)
        XCTAssertEqual(ShotFiles.list(cave: cave).map(\.lastPathComponent), ["aaa-newer.cvcap", "zzz-older.cvcap"])
    }

    func testRaggedMeshIsRefused() {
        let bad = MeshPart(positions: [SIMD3(0, 0, 0), SIMD3(1, 0, 0), SIMD3(0, 1, 0)],
                           normals: [SIMD3(0, 0, 1)], indices: [0, 1, 2], transform: matrix_identity_float4x4)
        let st = Station(id: "a", label: "A1", x: 0, y: 0, z: 0)
        let job = ShotJob(caveId: "test-cave-ragged", over: st, parts: [bad], keyframes: [], startCamera: .zero,
                          tracking: "normal", systemVersion: "26.0", compass: .none)
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: "test-cave-ragged")) }
        guard case .failure(let f) = job.write() else { return XCTFail("ragged mesh written") }
        XCTAssertEqual(f.message, "The shot could not be written.")
    }
}
