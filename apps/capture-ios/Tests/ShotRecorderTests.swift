import ARKit
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

    /// Pass flags: a new shot writes them to the header; a pass stores them in its own pass record.
    func testFlagsReachTheHeaderAndThePass() throws {
        let cave = "test-cave-flags"
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: cave)) }
        let a = Station(id: "a", label: "A1", x: 0, y: 0, z: 0)
        let kf = ShotKeyframe(pose: matrix_identity_float4x4, intrinsics: matrix_identity_float3x3, w: 1, h: 1, jpeg: Data([1]))
        let compass = CompassReading(trueNorth: true, accuracyDeg: 5, headingDeg: 0)
        let first = ShotJob(caveId: cave, over: a, parts: [Self.tri], keyframes: [kf], startCamera: .zero, tracking: "normal",
                            systemVersion: "27", compass: compass, flags: ["gimbal"])
        guard case .success(let url) = first.write() else { return XCTFail("shot not written") }
        XCTAssertEqual(try Self.header(of: Data(contentsOf: url))["flags"] as? [String], ["gimbal"])
        let pass = ShotJob(caveId: cave, over: a, parts: [Self.tri], keyframes: [kf], startCamera: .zero, tracking: "normal",
                           systemVersion: "27", compass: compass, flags: ["gimbal", "low battery"])
        guard case .success = pass.writePass(into: url, sameSession: true, transform: nil) else { return XCTFail("pass not written") }
        let h = try Self.header(of: Data(contentsOf: url))
        let passes = h["passes"] as? [[String: Any]]
        XCTAssertEqual(passes?.count, 2)
        XCTAssertEqual(passes?[0]["flags"] as? [String], ["gimbal"])
        XCTAssertEqual(passes?[1]["flags"] as? [String], ["gimbal", "low battery"])
        XCTAssertEqual(h["flags"] as? [String], ["gimbal"])
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
        // a parent that cannot be lined up never blocks Pause: the pass becomes a separate shot over the station
        XCTAssertEqual(ShotRecorder.saveMode(hasParent: true, sessionToShot: nil), .separateShot)
        XCTAssertEqual(ShotRecorder.saveMode(hasParent: true, sessionToShot: ShotRecorder.identity), .sameSession)
        XCTAssertEqual(ShotRecorder.saveMode(hasParent: true, sessionToShot: fit), .cards(fit))
    }

    /// Start/Resume when ready; Pause and Stop while capturing; Resume and Stop while paused. ARKit's live mesh
    /// shows only while capturing (paused, the scan shows as its ghost).
    func testButtonsAndLiveMeshPerPhase() {
        let u = URL(fileURLWithPath: "/tmp/x.cvcap")
        XCTAssertEqual(ShotRecorder.controls(.ready, hasParent: false), [.start])
        XCTAssertEqual(ShotRecorder.controls(.ready, hasParent: true), [.resume])
        XCTAssertEqual(ShotRecorder.controls(.capturing, hasParent: true), [.pause, .stop])
        XCTAssertEqual(ShotRecorder.controls(.saving, hasParent: true), [])
        XCTAssertEqual(ShotRecorder.controls(.saved(u), hasParent: true), [.viewShot, .resume, .stop])
        XCTAssertEqual(ShotRecorder.controls(.failed("x"), hasParent: true), [.resume])
        XCTAssertTrue(ShotRecorder.showsLiveMesh(.capturing))
        for p: ShotRecorder.Phase in [.ready, .saving, .saved(u), .failed("x")] { XCTAssertFalse(ShotRecorder.showsLiveMesh(p)) }
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
        XCTAssertNil(ShotFiles.parent(cave: cave, station: a))
        let ua = write(a)
        _ = write(b)
        XCTAssertEqual(ShotFiles.parent(cave: cave, station: a), ua)
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
        XCTAssertEqual(same, ShotJob.PassSaved(url: ua, newShotBecause: nil))
        let h = try Self.header(of: try Data(contentsOf: ua))
        XCTAssertEqual((h["passes"] as? [[String: Any]])?.count, 2)
        // the keyframes live beside the header on the wire: count them through the summary
        let sum = Core.summary(bundle: try Data(contentsOf: ua), pair: nil)
        XCTAssertEqual(sum["keyframes"] as? Int, 1)
        XCTAssertEqual(sum["passes"] as? Int, 2)
        // still the station's one shot: the pass did not make a new file
        XCTAssertEqual(ShotFiles.parent(cave: cave, station: a), ua)
        XCTAssertEqual(ShotFiles.list(cave: cave).count, 2)

        // a pass into a file that is gone is saved as a new shot (and the missing file is not made)
        let missing = ShotFiles.folder(cave: cave).appendingPathComponent("nope.cvcap")
        let lost = ShotJob(caveId: cave, over: a, parts: [Self.tri], keyframes: [], startCamera: .zero, tracking: "normal",
                           systemVersion: "27", compass: .none)
        guard case .success(let saved) = lost.writePass(into: missing, sameSession: true, transform: nil) else { return XCTFail("pass into nothing") }
        XCTAssertEqual(saved.newShotBecause, "cannot open the file")
        XCTAssertNotEqual(saved.url, missing)
        XCTAssertFalse(FileManager.default.fileExists(atPath: missing.path))
        XCTAssertEqual(Core.summary(bundle: try Data(contentsOf: saved.url), pair: nil)["vertices"] as? Int, 3)
        try FileManager.default.removeItem(at: saved.url)
        // an unwritable pass (a non-finite transform) is a plain failure and leaves the parent as it was
        let kept = try Data(contentsOf: ua)
        let nan = [Double](repeating: .nan, count: 16)
        guard case .failure(let g) = ShotJob(caveId: cave, over: a, parts: [Self.tri], keyframes: [], startCamera: .zero, tracking: "normal",
                                             systemVersion: "27", compass: .none).writePass(into: ua, sameSession: false, transform: nan)
        else { return XCTFail("a non-finite pass was written") }
        XCTAssertEqual(g.message, "The shot could not be written.")
        XCTAssertEqual(try Data(contentsOf: ua), kept)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: ShotFiles.folder(cave: cave).path).filter { $0.hasSuffix(".tmp") }, [])
    }

    /// The parent rule over header summaries, newest first: the station's newest shot decides.
    func testPickParentRule() {
        let a = Station(id: "a", label: "A1", x: 0, y: 0, z: 0)
        let u1 = URL(fileURLWithPath: "/s/1.cvcap"), u2 = URL(fileURLWithPath: "/s/2.cvcap"), u3 = URL(fileURLWithPath: "/s/3.cvcap")
        let ok: [String: Any] = ["station_id": "a", "vertices": 3]
        let other: [String: Any] = ["station_id": "b", "vertices": 3]
        let broken: [String: Any] = ["error": "Truncated", "unreadable": ["shot_id": "x", "station_id": "a", "station_label": "A1", "waiting": false]]
        let brokenOther: [String: Any] = ["error": "NewerFormat(2)", "unreadable": ["station_id": "b", "station_label": "B1", "waiting": false]]
        let junk: [String: Any] = ["error": "NotACapture"]
        XCTAssertEqual(ShotFiles.pickParent([(URL, [String: Any])](), station: a), .none)
        XCTAssertEqual(ShotFiles.pickParent([(u1, other), (u2, ok)], station: a), .parent(u2))
        // newest over the station is unreadable: a new shot, even with an older readable one behind it
        XCTAssertEqual(ShotFiles.pickParent([(u1, broken), (u2, ok)], station: a), .unreadable(u1, reason: "Truncated"))
        // files that do not say whose they are, or are another station's, are passed over
        XCTAssertEqual(ShotFiles.pickParent([(u1, junk), (u2, brokenOther), (u3, ok)], station: a), .parent(u3))
        XCTAssertEqual(ShotFiles.pickParent([(u1, ok), (u2, broken)], station: a), .parent(u1))
        // a waiting shot over a label the station now carries counts, readable or not
        let waitingBroken: [String: Any] = ["error": "Truncated", "unreadable": ["station_id": "", "station_label": "a 1", "waiting": true]]
        XCTAssertEqual(ShotFiles.pickParent([(u1, waitingBroken)], station: a), .unreadable(u1, reason: "Truncated"))
        // a waiting station has no parent
        XCTAssertEqual(ShotFiles.pickParent([(u1, ok)], station: .waiting(label: "A1")), .none)
        XCTAssertEqual(ShotJob.unreadableParentLine(u1, reason: "Truncated"), "parent 1 unreadable (Truncated): this capture saves as a new shot")
    }

    /// A station's shot that cannot be read is found from its header when the screen opens, and a pass meant for
    /// it (unreadable between open and save) is saved as a new shot; the unreadable file is never touched.
    func testUnreadableParentSavesANewShot() throws {
        let cave = "test-cave-unreadable"
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: cave)) }
        let a = Station(id: "a", label: "A1", x: 0, y: 0, z: 0)
        let compass = CompassReading(trueNorth: true, accuracyDeg: 5, headingDeg: 0)
        let kf = ShotKeyframe(pose: matrix_identity_float4x4, intrinsics: matrix_identity_float3x3, w: 1, h: 1, jpeg: Data([5, 5, 5, 5]))
        func job() -> ShotJob {
            ShotJob(caveId: cave, over: a, parts: [Self.tri], keyframes: [kf], startCamera: .zero, tracking: "normal",
                    systemVersion: "27", compass: compass)
        }
        guard case .success(let ua) = job().write() else { return XCTFail("not written") }
        XCTAssertEqual(ShotFiles.parentPick(cave: cave, station: a), .parent(ua))
        let whole = try Data(contentsOf: ua)

        for (name, bytes, reason) in [
            ("cut", whole.prefix(whole.count - 2), "Truncated"),
            ("newer", { var d = whole; d[6] = 9; d[7] = 0; return d }(), "NewerFormat(9)"),
        ] {
            try bytes.write(to: ua)
            // at open: the header check names it, and it is not the parent
            XCTAssertEqual(ShotFiles.parentPick(cave: cave, station: a), .unreadable(ua, reason: reason), name)
            XCTAssertNil(ShotFiles.parent(cave: cave, station: a), name)
            // at save: the pass becomes a new shot and the unreadable file is left exactly as it was
            guard case .success(let s) = job().writePass(into: ua, sameSession: true, transform: nil) else { return XCTFail(name) }
            XCTAssertEqual(s.newShotBecause, reason, name)
            XCTAssertNotEqual(s.url, ua, name)
            XCTAssertEqual(try Data(contentsOf: ua), bytes, name)
            let sum = Core.summary(bundle: try Data(contentsOf: s.url), pair: nil)
            XCTAssertNil(sum["error"], name)
            XCTAssertEqual(sum["keyframes"] as? Int, 1, name)
            XCTAssertEqual(sum["station_id"] as? String, "a", name)
            // the new shot is the station's newest: it is the next capture's parent
            XCTAssertEqual(ShotFiles.parentPick(cave: cave, station: a), .parent(s.url), name)
            try FileManager.default.removeItem(at: s.url)
        }

        // a header that reads but a mesh that does not decode: found only at save, same fallback
        var badMesh = whole
        // the triangle's last index (u32) is the last word before the photo: point it past the vertices
        let at = badMesh.count - 4 - 4
        badMesh.replaceSubrange(at..<(at + 4), with: [99, 0, 0, 0])
        try badMesh.write(to: ua)
        XCTAssertEqual(ShotFiles.parentPick(cave: cave, station: a), .parent(ua))
        guard case .success(let s) = job().writePass(into: ua, sameSession: true, transform: nil) else { return XCTFail("bad mesh") }
        XCTAssertEqual(s.newShotBecause, "BadMesh")
        XCTAssertEqual(try Data(contentsOf: ua), badMesh)
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: ShotFiles.folder(cave: cave).path).filter { $0.hasSuffix(".tmp") }, [])
    }

    /// A pass streamed into the parent keeps every earlier photo and adds its own, pass after pass.
    func testPassesStreamIntoTheParent() throws {
        let cave = "test-cave-stream"
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: cave)) }
        let a = Station(id: "a", label: "A1", x: 0, y: 0, z: 0)
        let compass = CompassReading(trueNorth: true, accuracyDeg: 5, headingDeg: 0)
        func job(_ byte: UInt8) -> ShotJob {
            let kf = ShotKeyframe(pose: matrix_identity_float4x4, intrinsics: matrix_identity_float3x3, w: 1, h: 1,
                                  jpeg: Data(repeating: byte, count: 1000))
            return ShotJob(caveId: cave, over: a, parts: [Self.tri], keyframes: [kf, kf], startCamera: .zero, tracking: "normal",
                           systemVersion: "27", compass: compass)
        }
        guard case .success(let u) = job(1).write() else { return XCTFail("not written") }
        let first = try Data(contentsOf: u)
        guard case .success(let p2) = job(2).writePass(into: u, sameSession: true, transform: nil),
              case .success(let p3) = job(3).writePass(into: u, sameSession: false, transform: ShotRecorder.identity)
        else { return XCTFail("passes not written") }
        XCTAssertEqual([p2.url, p3.url], [u, u])
        XCTAssertNil(p2.newShotBecause)
        let out = try Data(contentsOf: u)
        let sum = Core.summary(bundle: out, pair: nil)
        XCTAssertEqual(sum["passes"] as? Int, 3)
        XCTAssertEqual(sum["keyframes"] as? Int, 6)
        // the photos sit back to back at the end of the file: first pass's, then each new pass's
        XCTAssertEqual(out.suffix(6000), Data(repeating: 1, count: 2000) + Data(repeating: 2, count: 2000) + Data(repeating: 3, count: 2000))
        XCTAssertEqual(first.suffix(2000), out.suffix(6000).prefix(2000))
    }

    func testOutlineRibbonsAndLineUpRule() {
        let r = GhostView.ribbon(outlines: [[SIMD3(0, 0, 0), SIMD3(1, 0, 0), SIMD3(1, 0, 1)]], width: 0.02)
        XCTAssertEqual(r.indices.count, 2 * 4 * 3)
        XCTAssertTrue(r.positions.allSatisfy { $0.x.isFinite && $0.y.isFinite && $0.z.isFinite })
        // an upright segment (parallel to up) and a repeated point make no NaN either
        let v = GhostView.ribbon(outlines: [[SIMD3(0, 0, 0), SIMD3(0, 1, 0), SIMD3(0, 1, 0)]], width: 0.02)
        XCTAssertEqual(v.indices.count, 1 * 4 * 3)
        XCTAssertTrue(v.positions.allSatisfy { $0.x.isFinite && $0.y.isFinite && $0.z.isFinite })
        let t16 = [Double](repeating: 0, count: 16)
        // without "ok" (an older core): the 5 cm rule
        let ok: [String: Any] = ["transform": t16, "yaw_rad": 0.1, "worst_m": 0.03, "misfits": [["id": 2, "m": 0.03]]]
        XCTAssertNotNil(ShotRecorder.lineUp(fit: ok).transform)
        let bad: [String: Any] = ["transform": t16, "yaw_rad": 0.1, "worst_m": 0.12, "misfits": [["id": 2, "m": 0.01], ["id": 12, "m": 0.12]]]
        let r2 = ShotRecorder.lineUp(fit: bad)
        XCTAssertNil(r2.transform)
        XCTAssertTrue(r2.message.contains("FORWARD"), r2.message)
        XCTAssertTrue(r2.message.contains("12 cm"), r2.message)
        XCTAssertTrue(r2.message.contains("Pause will save this as a separate scan"), r2.message)
        // "ok" decides: 7 cm accepted when the card's own limit allows it
        let far: [String: Any] = ["ok": true, "transform": t16, "worst_m": 0.07, "worst_ratio": 0.9,
                                  "misfits": [["id": 2, "m": 0.07]], "limits": [["id": 2, "m": 0.078]]]
        XCTAssertNotNil(ShotRecorder.lineUp(fit: far).transform)
        // ... and refuses 6 cm when it does not; the card named is the worst by ratio: CURRENT (6 of 4 cm), not FORWARD (7.5 of 10)
        let near: [String: Any] = ["ok": false, "transform": t16, "worst_m": 0.06, "worst_ratio": 1.5,
                                   "misfits": [["id": 2, "m": 0.06], ["id": 12, "m": 0.075]],
                                   "limits": [["id": 2, "m": 0.04], ["id": 12, "m": 0.1]]]
        let r3 = ShotRecorder.lineUp(fit: near)
        XCTAssertNil(r3.transform)
        XCTAssertTrue(r3.message.hasPrefix("\(StationCard(rawValue: 2)!.word) doesn't match where it was (6 cm)."), r3.message)
        // "ok" true with no usable transform is still refused
        XCTAssertNil(ShotRecorder.lineUp(fit: ["ok": true, "worst_m": 0.01]).transform)
    }

    func testGhostPlacementIsTheInverseOfTheLineUp() throws {
        XCTAssertNil(ShotRecorder.ghostPlacement(nil))
        XCTAssertEqual(ShotRecorder.ghostPlacement(ShotRecorder.identity), matrix_identity_float4x4)
        // a quarter turn about +y then a shift (2, 0, 3): session -> shot
        let fit: [Double] = [0, 0, -1, 0, 0, 1, 0, 0, 1, 0, 0, 0, 2, 0, 3, 1]
        let g = try XCTUnwrap(ShotRecorder.ghostPlacement(fit))
        // the shot's point where the session's origin lands goes back to the session's origin
        let back = g * SIMD4<Float>(2, 0, 3, 1)
        XCTAssertLessThan(simd_length(SIMD3(back.x, back.y, back.z)), 1e-5)
        XCTAssertNil(ShotRecorder.ghostPlacement([1, 2, 3]))
    }

    func testCardCheckRunsOnlyWhenSettledCardsChange() {
        let a: [Int: SIMD3<Double>] = [2: SIMD3(0, 0, 0), 12: SIMD3(1, 0, 0)]
        XCTAssertFalse(ShotRecorder.cardsChanged(last: a, now: a))
        XCTAssertFalse(ShotRecorder.cardsChanged(last: a, now: [2: SIMD3(0.005, 0, 0), 12: SIMD3(1, 0, 0)]))
        XCTAssertTrue(ShotRecorder.cardsChanged(last: a, now: [2: SIMD3(0.03, 0, 0), 12: SIMD3(1, 0, 0)]))
        XCTAssertTrue(ShotRecorder.cardsChanged(last: a, now: [2: SIMD3(0, 0, 0)]))
        XCTAssertTrue(ShotRecorder.cardsChanged(last: [:], now: a))
        // the offsets warn once per card, and only past 5 cm
        let w = ShotRecorder.moved(offsets: [["id": 2, "m": 0.07], ["id": 12, "m": 0.02], ["id": 26, "m": 0.2]], warned: [.rear])
        XCTAssertEqual(w.map(\.card), [.current])
        XCTAssertEqual(w.first?.m ?? 0, 0.07, accuracy: 1e-9)
    }

    /// Gimbal mode: Start refuses with the check's reason when the gimbal is not connected; nothing starts.
    @MainActor func testGimbalStartRefusesWhenNotConnected() {
        let rec = ShotRecorder(), g = FakeGimbal()
        g.st.connected = false
        rec.gimbal = g
        rec.gimbalMode = true
        rec.start()
        XCTAssertEqual(rec.message, "Gimbal not connected.")
        guard case .ready = rec.phase else { return XCTFail("phase \(rec.phase)") }
        XCTAssertTrue(g.runs.isEmpty)
        XCTAssertEqual(rec.gimbalLine, "Gimbal: connecting…")
        rec.gimbalMode = false
    }

    /// A gimbal still busy with a plan (moving, held after a Pause, going home) is sent home before a new pass.
    func testGimbalBusy() {
        for (state, busy) in [("idle", false), ("done", false), ("error", false), ("moving", true), ("holding", true), ("homing", true)] {
            XCTAssertEqual(ShotRecorder.gimbalBusy(GimbalStatus(connected: true, state: state)), busy, state)
        }
    }

    /// Home (pivot and heading) is taken once, when the runner reports it has entered target mode.
    func testHomeTakenOnceTheRunnerHasEntered() {
        var s = GimbalStatus(connected: true, state: "moving", steps: 11, mountOK: true)
        XCTAssertFalse(ShotRecorder.takesHome(taken: false, s))
        s.entered = true
        XCTAssertTrue(ShotRecorder.takesHome(taken: false, s))
        XCTAssertFalse(ShotRecorder.takesHome(taken: true, s))
        XCTAssertTrue(GimbalStatus(["entered": true]).entered)
        XCTAssertFalse(GimbalStatus([:]).entered)
    }

    func testGimbalInputsFromARKit() {
        XCTAssertEqual(ShotRecorder.gimbalTracking(.normal), .normal)
        XCTAssertEqual(ShotRecorder.gimbalTracking(.limited(.initializing)), .limited)
        XCTAssertEqual(ShotRecorder.gimbalTracking(.limited(.relocalizing)), .limited)
        XCTAssertEqual(ShotRecorder.gimbalTracking(.notAvailable), .lost)
        XCTAssertEqual(ShotRecorder.lux(nil), 0)
        XCTAssertEqual(ShotRecorder.lux(850), 850)
        XCTAssertEqual(ShotRecorder.batteryFraction(-1), 1)
        XCTAssertEqual(ShotRecorder.batteryFraction(0.5), 0.5)
        XCTAssertEqual(ShotRecorder.coverageCells, 11 * 36)
    }

    @MainActor func testGimbalLine() {
        var s = GimbalStatus()
        XCTAssertEqual(ShotRecorder.gimbalLine(s, connecting: true, stage: nil, holdReason: nil), "Gimbal: connecting…")
        s.state = "error"; s.error = "no gimbal found"
        XCTAssertEqual(ShotRecorder.gimbalLine(s, connecting: false, stage: nil, holdReason: nil), "Gimbal not found: no gimbal found")
        s = GimbalStatus(connected: true, state: "idle", mountOK: true)
        XCTAssertEqual(ShotRecorder.gimbalLine(s, connecting: false, stage: nil, holdReason: nil), "Gimbal connected")
        s.mountOK = false
        XCTAssertEqual(ShotRecorder.gimbalLine(s, connecting: false, stage: nil, holdReason: nil), "Gimbal connected: put the phone upright")
        s = GimbalStatus(connected: true, state: "moving", step: 4, steps: 11, mountOK: true)
        XCTAssertEqual(ShotRecorder.gimbalLine(s, connecting: false, stage: .sweep, holdReason: nil), "Sweeping 4/11")
        XCTAssertEqual(ShotRecorder.gimbalLine(s, connecting: false, stage: .sweep, holdReason: "too dark"), "Holding: too dark")
        XCTAssertEqual(ShotRecorder.gimbalLine(s, connecting: false, stage: .fill, holdReason: nil), "Filling holes…")
    }
}
