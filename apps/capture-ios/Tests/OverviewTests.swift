import SceneKit
import XCTest
@testable import CaveCAD_Capture

final class OverviewTests: XCTestCase {
    /// A picture is made again only when missing or older than its shot.
    func testThumbnailStaleness() {
        let t0 = Date(timeIntervalSince1970: 1_000), t1 = Date(timeIntervalSince1970: 2_000)
        XCTAssertTrue(ShotThumbnail.isStale(shotModified: t0, thumbModified: nil))
        XCTAssertTrue(ShotThumbnail.isStale(shotModified: nil, thumbModified: nil))
        XCTAssertTrue(ShotThumbnail.isStale(shotModified: t1, thumbModified: t0), "a Resume rewrote the shot")
        XCTAssertFalse(ShotThumbnail.isStale(shotModified: t0, thumbModified: t1))
        XCTAssertFalse(ShotThumbnail.isStale(shotModified: t0, thumbModified: t0))
        XCTAssertFalse(ShotThumbnail.isStale(shotModified: nil, thumbModified: t0))
    }

    /// The picture sits beside its shot; the list never shows it; deleting the shot deletes it.
    func testDeleteRemovesThumbnailAndListSkipsIt() throws {
        let cave = "test-cave-thumbs"
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: cave)) }
        let a = try ShotFiles.save(Data([1]), cave: cave, shot: "a")
        let b = try ShotFiles.save(Data([2]), cave: cave, shot: "b")
        let ta = ShotFiles.thumbnail(of: a), tb = ShotFiles.thumbnail(of: b)
        XCTAssertEqual(ta.lastPathComponent, "a.thumb.png")
        XCTAssertEqual(ta.deletingLastPathComponent(), a.deletingLastPathComponent())
        try Data([9]).write(to: ta)
        try Data([9]).write(to: tb)
        XCTAssertEqual(Set(ShotFiles.list(cave: cave).map(\.lastPathComponent)), ["a.cvcap", "b.cvcap"])
        XCTAssertTrue(ShotFiles.delete(a))
        XCTAssertFalse(FileManager.default.fileExists(atPath: ta.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: tb.path))
        XCTAssertFalse(ShotFiles.delete(tb), "only .cvcap files are deleted this way")
        // a picture left without its shot goes with delete-all too
        try Data([9]).write(to: ShotFiles.thumbnail(of: a))
        XCTAssertEqual(ShotFiles.deleteAll(cave: cave), 1)
        let left = try FileManager.default.contentsOfDirectory(atPath: ShotFiles.folder(cave: cave).path)
        XCTAssertEqual(left, [])
    }

    /// Offsets are taken from the first placed shot's station, in Double before narrowing.
    func testOverallOffsetsFromTheFirstStation() {
        let o = Station(id: "o", label: "A1", x: 500_000, y: 4_000_000, z: 300)
        let s = Station(id: "s", label: "A2", x: 500_003.25, y: 4_000_001.5, z: 299)
        XCTAssertEqual(Overall.offset(s, from: o), SIMD3<Float>(3.25, 1.5, -1))
        XCTAssertEqual(Overall.offset(o, from: o), .zero)
        // the origin is the station of the earliest shot in the list that could be placed, whatever the order
        let parts = [Overall.Part(index: 3, over: s, points: [0, 0, 0]), Overall.Part(index: 1, over: o, points: [1, 0, 0])]
        XCTAssertEqual(Overall.origin(parts), o)
        XCTAssertEqual(Overall.origin(parts.reversed()), o)
        XCTAssertNil(Overall.origin([]))
        // bounds: each shot's points moved to its station
        let box = Overall.bounds(parts, origin: o)
        XCTAssertEqual(box?.min, SIMD3<Float>(1, 0, -1))
        XCTAssertEqual(box?.max, SIMD3<Float>(3.25, 1.5, 0))
    }

    /// Each shot's colour depends on its place in the list alone; the first ten are all different.
    func testColoursStablePerShotOrder() {
        for i in 0..<30 { XCTAssertEqual(Overall.colour(i), Overall.colour(i)) }
        let first = (0..<10).map(Overall.colour)
        for i in 0..<10 { for j in (i + 1)..<10 { XCTAssertGreaterThan(simd_distance(first[i], first[j]), 0.15, "\(i) vs \(j)") } }
        for i in [10, 11, 57, 1000] {
            let c = Overall.colour(i)
            XCTAssertTrue(all(c .>= 0) && all(c .<= 1), "\(c)")
        }
    }

    /// Stations are marked within 5 m of the shots' box, and always the shots' own.
    func testMarkedStations() {
        let o = Station(id: "o", label: "A1", x: 10, y: 10, z: 0)
        let near = Station(id: "n", label: "A2", x: 14, y: 10, z: 0)
        let far = Station(id: "f", label: "A3", x: 40, y: 10, z: 0)
        let own = Station(id: "w", label: "A4", x: 60, y: 10, z: 0)
        let m = Overall.marked([o, near, far, own], origin: o, min: SIMD3(-1, -1, -1), max: SIMD3(1, 1, 1), always: ["w"])
        XCTAssertEqual(m.map(\.id), ["o", "n", "w"])
    }

    /// The scene puts each shot at its station (SceneKit y up: survey (x, z, -y)) and marks stations.
    func testOverallScene() {
        let o = Station(id: "o", label: "A1", x: 0, y: 0, z: 0)
        let s = Station(id: "s", label: "A2", x: 2, y: 3, z: 1)
        let scene = Overall.scene(parts: [Overall.Part(index: 0, over: o, points: [0, 0, 0, 1, 0, 0]),
                                          Overall.Part(index: 1, over: s, points: [0, 0, 0])],
                                  origin: o, stations: [o, s])
        let shot = scene.rootNode.childNode(withName: "shot-1", recursively: false)
        XCTAssertEqual(shot?.simdPosition, SIMD3<Float>(2, 1, -3))
        XCTAssertNotNil(scene.rootNode.childNode(withName: "station-o", recursively: false))
        XCTAssertNotNil(scene.rootNode.childNode(withName: "station-s", recursively: false))
    }

    /// Survey box to SceneKit box: y up, north is -z.
    func testSceneBox() {
        let b = PointCloud.sceneBox(min: SIMD3(-1, -2, -3), max: SIMD3(1, 2, 3))
        XCTAssertEqual(b.min, SIMD3<Float>(-1, -3, -2))
        XCTAssertEqual(b.max, SIMD3<Float>(1, 3, 2))
        XCTAssertNil(PointCloud.bounds([1, 2]))
    }

    /// A picture comes out at 80 pt @3x.
    func testRenderMakesAPicture() throws {
        guard let device = MTLCreateSystemDefaultDevice() else { throw XCTSkip("no Metal") }
        var pts: [Float] = []
        for i in 0..<400 { let t = Float(i) * 0.05; pts += [cos(t) * 2, sin(t) * 2, Float(i % 20) * 0.1] }
        let png = try XCTUnwrap(ShotThumbnail.render(points: pts, with: SCNRenderer(device: device, options: nil)))
        let img = try XCTUnwrap(UIImage(data: png))
        XCTAssertEqual(img.size.width * img.scale, 240)
        XCTAssertNil(ShotThumbnail.render(points: [], with: SCNRenderer(device: device, options: nil)))
    }
}
