import simd
import XCTest
@testable import CaveCAD_Capture

/// Card chaining on the phone: the core's chain result read back, the fix applied, and how the list shows it.
final class ChainTests: XCTestCase {
    let a3 = Station(id: "a3", label: "A3", x: 10, y: 20, z: 5)
    let a4 = Station(id: "a4", label: "A4", x: 14, y: 23, z: 4.5)

    /// A compass shot whose frame is turned `yawDeg` (counter-clockwise from above) and shifted `at` from the
    /// survey, seeing `cards` (glyph id, true survey point); a three-point mesh. Written to the cave's folder.
    func shot(_ cave: String, _ name: String, label: String, id: String, yawDeg: Double, at: SIMD3<Double>,
              cards: [(Int, SIMD3<Double>)]) throws -> URL {
        let a = yawDeg * .pi / 180, c = cos(a), s = sin(a)
        func cap(_ p: SIMD3<Double>) -> [Double] {
            let d = p - at
            return [c * d.x + s * d.y, d.z, -(-s * d.x + c * d.y)]
        }
        let header: [String: Any] = [
            "shot_id": name, "cave_id": cave, "station_id": id, "station_label": label, "captured_at_ms": 0,
            "device": "t", "instrument_height_m": 0.3, "start_camera": cap(at), "taps": [],
            "compass": ["true_north": true, "accuracy_deg": 5.0], "tracking": "normal", "arkit_version": "26.1",
            "gravity": [0, -1, 0], "keyframes": [],
            "cards": cards.map { ["id": $0.0, "word": "", "point": cap($0.1), "corners": [], "sightings": 9, "spread_m": 0.01] },
        ]
        let h = try JSONSerialization.data(withJSONObject: header)
        let b = Core.encodeShot(header: h, positions: [0, 0, -1, 1, 0, -1, 0, 1, -1], normals: [Float](repeating: 0, count: 9),
                                indices: [0, 1, 2], jpegs: Data())
        XCTAssertFalse(b.isEmpty)
        return try ShotFiles.save(b, cave: cave, shot: name)
    }

    func testCoreChainsTwoNeighboursAndCorrectsTheCompass() throws {
        let cave = "test-cave-chains"
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: cave)) }
        let p3 = SIMD3(a3.x, a3.y, a3.z), p4 = SIMD3(a4.x, a4.y, a4.z)
        let s = try shot(cave, "s", label: "A3", id: "a3", yawDeg: 0, at: p3 + [0, 0, 0.3], cards: [(2, p3), (12, p4)])
        let t = try shot(cave, "t", label: "A4", id: "a4", yawDeg: 10, at: p4 + [0, 0, 0.3], cards: [(2, p4), (26, p3)])
        let r = Core.chains(stations: [a3, a4], shots: [s, t])
        XCTAssertEqual(r.logLine, "chains: 2 shots, 1 chains, 2 placed by chain, 0 flagged")
        let pt = try XCTUnwrap(r.places[1])
        XCTAssertTrue(pt.isChained)
        XCTAssertEqual(pt.joined, ["A3"])
        XCTAssertEqual(pt.fixYawDeg, 10, accuracy: 0.5, "the chain turns the 10° compass error back")
        XCTAssertEqual(pt.pair?.first, a4.x)
        XCTAssertLessThan(simd_length(try XCTUnwrap(pt.spot)), 0.01)
        XCTAssertEqual(r.chains.first?.span, "A3–A4")

        // the list: "over A4 · chained to A3"
        let saved = withChains([SavedShot(url: s, made: nil, over: a3, forward: nil, compassLabel: nil),
                                SavedShot(url: t, made: nil, over: a4, forward: nil, compassLabel: nil)], r)
        XCTAssertEqual(saved[1].title, "over A4 · chained to A3")
        XCTAssertEqual(saved[1].drawPair, pt.pair)
        XCTAssertTrue(saved[1].misfitLines.isEmpty)
    }

    func testWaitingChainFloatsWithItsName() throws {
        let cave = "test-cave-chains-float"
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: cave)) }
        let a7 = SIMD3<Double>(100, 50, -3), a8 = SIMD3<Double>(103, 54, -3.5)
        let s = try shot(cave, "s", label: "A7", id: "", yawDeg: 0, at: a7 + [0, 0, 0.3], cards: [(2, a7), (12, a8)])
        let t = try shot(cave, "t", label: "A8", id: "", yawDeg: 0, at: a8 + [0, 0, 0.3], cards: [(2, a8), (26, a7)])
        let r = Core.chains(stations: [a3], shots: [s, t])
        let p = try XCTUnwrap(r.places[1])
        XCTAssertTrue(p.isFloating)
        XCTAssertFalse(p.placed)
        XCTAssertEqual(p.pair, [0, 0, 0, 0, 0, 0])
        XCTAssertEqual(try XCTUnwrap(p.spot).x, 3, accuracy: 0.01)
        let saved = withChains([SavedShot(url: s, made: nil, over: nil, forward: nil, compassLabel: nil, waitingLabel: "A7"),
                                SavedShot(url: t, made: nil, over: nil, forward: nil, compassLabel: nil, waitingLabel: "A8")], r)
        XCTAssertEqual(saved[0].title, "chain A7–A8 · waiting for station")
        XCTAssertTrue(saved[0].canOpen)
    }

    func testFixTurnsCounterClockwiseThenShifts() {
        let c = ChainPlace(source: "chain", placed: true, fixYawDeg: 90, fixShift: [1, 0, 0])
        let q = c.apply(SIMD3<Double>(1, 0, 0.5))
        XCTAssertEqual(q.x, 1, accuracy: 1e-9)
        XCTAssertEqual(q.y, 1, accuracy: 1e-9)
        XCTAssertEqual(q.z, 0.5, accuracy: 1e-9)
        let pts = c.apply([Float(1), 0, 0.5, 0, 2, 0])
        XCTAssertEqual(pts[0], 1, accuracy: 1e-6); XCTAssertEqual(pts[1], 1, accuracy: 1e-6)
        XCTAssertEqual(pts[3], -1, accuracy: 1e-6); XCTAssertEqual(pts[4], 0, accuracy: 1e-6)
        XCTAssertEqual(ChainPlace(source: "compass", placed: true).apply([Float(1), 2, 3]), [1, 2, 3])
    }

    func testMisfitIsShownWithItsNumbers() {
        let f = ChainFlag(["kind": "leg_misfit", "with": "A4", "lidar_m": 4.92, "survey_m": 5.1, "off_m": 0.18, "limit_m": 0.153])
        XCTAssertEqual(f?.text(own: "A3"), "A3–A4 misfit: LiDAR 4.92 m, survey 5.10 m (off 0.18 m, limit 0.15 m)")
        XCTAssertTrue(f?.isMisfit ?? false)
        XCTAssertFalse(ChainFlag(kind: "one_point", with: "A4").isMisfit)
        var s = SavedShot(url: URL(fileURLWithPath: "/x.cvcap"), made: nil, over: a3, forward: nil, compassLabel: nil)
        s.chain = ChainPlace(source: "card+compass", placed: true, label: "A3", flags: [f!])
        XCTAssertEqual(s.misfitLines.count, 1)
        XCTAssertEqual(s.title, "over A3")
    }

    func testSpanSortsLabelsNaturally() {
        XCTAssertEqual(ChainInfo(k: 0, shots: [0, 1, 2], labels: ["A9", "A10", "A8"], placed: false).span, "A8–A10")
    }

    func testThumbnailPerTurnAndDeleteRemovesEvery() throws {
        let cave = "test-cave-chain-thumbs"
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: cave)) }
        let a = try ShotFiles.save(Data([1]), cave: cave, shot: "a")
        XCTAssertEqual(ShotFiles.thumbnail(of: a).lastPathComponent, "a.thumb.png")
        XCTAssertEqual(ShotFiles.thumbnail(of: a, yawDeg: -7).lastPathComponent, "a.y-7.thumb.png")
        try Data([9]).write(to: ShotFiles.thumbnail(of: a))
        try Data([9]).write(to: ShotFiles.thumbnail(of: a, yawDeg: 12))
        XCTAssertEqual(ShotFiles.thumbnails(of: a).count, 2)
        XCTAssertTrue(ShotFiles.delete(a))
        XCTAssertEqual(try FileManager.default.contentsOfDirectory(atPath: ShotFiles.folder(cave: cave).path), [])
    }

    func testOverallFloatingChainsStandEastOfThePlacedShots() {
        let origin = Station(id: "o", label: "A1", x: 500_000, y: 4_000_000, z: 300)
        let zero = Station(id: "", label: "", x: 0, y: 0, z: 0)
        let g = Overall.FloatingGroup(title: "chain A7–A8 · waiting for station",
                                      parts: [Overall.Part(index: 0, over: zero, points: [0, 0, 0, 3, 4, 0])],
                                      marks: [Station(id: "f", label: "A8", x: 3, y: 4, z: 0)])
        let moved = Overall.besideBox([g, g], origin: origin, box: (SIMD3(-2, -1, 0), SIMD3(10, 5, 1)))
        // first group starts 5 m east of the box, centred on it north-south
        XCTAssertEqual(moved[0].parts[0].over.x - origin.x, 15, accuracy: 1e-9)
        XCTAssertEqual(moved[0].parts[0].over.y - origin.y, 0, accuracy: 1e-9)
        XCTAssertEqual(moved[0].marks[0].x - origin.x, 18, accuracy: 1e-9)
        // the next one 5 m further on past the first's width (3 m)
        XCTAssertEqual(moved[1].parts[0].over.x - origin.x, 23, accuracy: 1e-9)
    }
}
