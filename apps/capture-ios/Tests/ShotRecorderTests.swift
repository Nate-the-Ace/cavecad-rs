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
                       "Compass unsure (±25°). A magnet or metal nearby may be pulling it; the shot will still save.")
        XCTAssertEqual(CompassReading(trueHeading: -1, magneticHeading: 0, accuracy: -1).warning,
                       "Compass unsure (±?°). A magnet or metal nearby may be pulling it; the shot will still save.")
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

    func testDeleteShotsOnlyInsideTheShotsFolder() throws {
        let cave = "test-cave-delete", other = "test-cave-delete-other"
        defer {
            try? FileManager.default.removeItem(at: ShotFiles.folder(cave: cave))
            try? FileManager.default.removeItem(at: ShotFiles.folder(cave: other))
        }
        let a = try ShotFiles.save(Data([1, 2, 3]), cave: cave, shot: "a")
        _ = try ShotFiles.save(Data([4, 5]), cave: cave, shot: "b")
        let keep = try ShotFiles.save(Data([6]), cave: other, shot: "c")
        XCTAssertEqual(ShotFiles.bytes(cave: cave), 5)
        XCTAssertTrue(ShotFiles.delete(a))
        XCTAssertEqual(ShotFiles.list(cave: cave).map(\.lastPathComponent), ["b.cvcap"])
        // nothing outside Documents/shots, and nothing that is not a shot
        XCTAssertFalse(ShotFiles.delete(Log.file))
        let notShot = ShotFiles.folder(cave: cave).appendingPathComponent("x.txt")
        try Data([1]).write(to: notShot)
        XCTAssertFalse(ShotFiles.delete(notShot))
        XCTAssertFalse(ShotFiles.delete(ShotFiles.folder(cave: cave).appendingPathComponent("../../capture.cvcap")))
        XCTAssertEqual(ShotFiles.deleteAll(cave: cave), 1)
        XCTAssertTrue(ShotFiles.list(cave: cave).isEmpty)
        XCTAssertEqual(ShotFiles.list(cave: other), [keep])
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

    func testPassGhostAndFitThroughTheCore() throws {
        let cave = "test-cave-pass"
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: cave)) }
        let over = Station(id: "a", label: "A1", x: 0, y: 0, z: 0)
        let job = ShotJob(caveId: cave, over: over, parts: [Self.tri], keyframes: [], startCamera: .zero, tracking: "normal",
                          systemVersion: "27", compass: CompassReading(trueNorth: true, accuracyDeg: 5, headingDeg: 0))
        guard case .success(let url) = job.write() else { return XCTFail("not written") }
        let old = try Data(contentsOf: url)
        let pass = try JSONSerialization.data(withJSONObject: [
            "captured_at_ms": 5, "aligned_by": "same_session", "transform": [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1],
            "replace": true, "tracking": "normal", "cards": [Any](), "keyframes": [Any](),
        ] as [String: Any])
        let out = Core.addPass(bundle: old, pass: pass, positions: [0, 0, -2, 1, 0, -2, 0, 1, -2], normals: [Float](repeating: 0, count: 9),
                               indices: [0, 1, 2], jpegs: Data())
        XCTAssertFalse(out.isEmpty)
        let g = try XCTUnwrap(Core.ghost(bundle: out, minOutlineM: 0))
        XCTAssertEqual(g.indices.count, 3)
        XCTAssertEqual(g.positions.count, 3)
        XCTAssertEqual(g.outlines.count, 1)
        XCTAssertNotNil(Core.cardsFit(bundle: out, cards: [], sameSession: true)["offsets"])
        XCTAssertNil(Core.ghost(bundle: Data([1, 2, 3])))
    }

    func testSaveModeFollowsHowTheSessionMapsIntoTheShot() {
        let fit: [Double] = [0, 0, -1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 2, 0, 3, 1]
        XCTAssertEqual(ShotRecorder.saveMode(hasParent: false, sessionToShot: nil), .newShot)
        XCTAssertEqual(ShotRecorder.saveMode(hasParent: false, sessionToShot: ShotRecorder.identity), .newShot)
        XCTAssertEqual(ShotRecorder.saveMode(hasParent: true, sessionToShot: nil), .refuse)
        XCTAssertEqual(ShotRecorder.saveMode(hasParent: true, sessionToShot: ShotRecorder.identity), .sameSession)
        XCTAssertEqual(ShotRecorder.saveMode(hasParent: true, sessionToShot: fit), .cards(fit))
    }

    func testParentShotAndSafeReplace() throws {
        let cave = "test-cave-parent"
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: cave)) }
        let a = Station(id: "a", label: "A1", x: 0, y: 0, z: 0), b = Station(id: "b", label: "B1", x: 0, y: 0, z: 0)
        func write(_ s: Station) -> URL {
            let job = ShotJob(caveId: cave, over: s, parts: [Self.tri], keyframes: [], startCamera: .zero, tracking: "normal",
                              systemVersion: "27", compass: CompassReading(trueNorth: true, accuracyDeg: 5, headingDeg: 0))
            guard case .success(let u) = job.write() else { XCTFail("not written"); return URL(fileURLWithPath: "/") }
            return u
        }
        XCTAssertNil(ShotFiles.parent(cave: cave, station: "a"))
        let ua = write(a)
        _ = write(b)
        XCTAssertEqual(ShotFiles.parent(cave: cave, station: "a"), ua)
        let before = try Data(contentsOf: ua)
        XCTAssertTrue(ShotFiles.replace(ua, with: Data([1, 2, 3])))
        XCTAssertEqual(try Data(contentsOf: ua), Data([1, 2, 3]))
        try before.write(to: ua)
        let gone = ShotFiles.folder(cave: "test-cave-missing").appendingPathComponent("x.cvcap")
        try? FileManager.default.removeItem(at: ShotFiles.folder(cave: "test-cave-missing"))
        XCTAssertFalse(ShotFiles.replace(gone, with: Data([9])))
        XCTAssertEqual(try Data(contentsOf: ua), before)
        // no stray temporary files are left beside the shots
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: ShotFiles.folder(cave: cave).path).filter { $0.hasSuffix(".tmp") }, [])

        // a same-session pass rewrites the parent with two passes
        let kf = ShotKeyframe(pose: matrix_identity_float4x4, intrinsics: matrix_identity_float3x3, w: 1, h: 1, jpeg: Data([5, 5]))
        let pass = ShotJob(caveId: cave, over: a, parts: [Self.tri], keyframes: [kf], startCamera: .zero, tracking: "normal",
                           systemVersion: "27", compass: CompassReading(trueNorth: true, accuracyDeg: 5, headingDeg: 0))
        guard case .success(let same) = pass.writePass(into: ua, sameSession: true, transform: nil) else { return XCTFail("pass not written") }
        XCTAssertEqual(same, ua)
        let h = try Self.header(of: try Data(contentsOf: ua))
        XCTAssertEqual((h["passes"] as? [[String: Any]])?.count, 2)
        // the keyframes live beside the header on the wire: count them through the summary
        let sum = Core.summary(bundle: try Data(contentsOf: ua), pair: nil)
        XCTAssertEqual(sum["keyframes"] as? Int, 1)
        XCTAssertEqual(sum["passes"] as? Int, 2)
        // still the station's one shot: the pass did not make a new file
        XCTAssertEqual(ShotFiles.parent(cave: cave, station: "a"), ua)
        XCTAssertEqual(ShotFiles.list(cave: cave).count, 2)

        // a pass into a file that cannot be read fails and writes nothing
        let missing = ShotFiles.folder(cave: cave).appendingPathComponent("nope.cvcap")
        let lost = ShotJob(caveId: cave, over: a, parts: [Self.tri], keyframes: [], startCamera: .zero, tracking: "normal",
                           systemVersion: "27", compass: .none)
        guard case .failure(let f) = lost.writePass(into: missing, sameSession: true, transform: nil) else { return XCTFail("pass into nothing") }
        XCTAssertEqual(f.message, "The earlier scan could not be read.")
    }
}
