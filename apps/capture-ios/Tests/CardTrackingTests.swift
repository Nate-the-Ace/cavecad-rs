import SceneKit
import XCTest
import simd
@testable import CaveCAD_Capture

final class CardTrackingTests: XCTestCase {
    static let k = simd_float3x3(columns: (SIMD3(1000, 0, 0), SIMD3(0, 1000, 0), SIMD3(960, 540, 1)))

    func testUnprojectCentrePixelIdentityCamera() {
        var cam = matrix_identity_float4x4
        cam.columns.3 = SIMD4(1, 2, 3, 1)
        let p = CardMath.unproject(pixel: SIMD2(960, 540), depth: 2, intrinsics: Self.k, camera: cam)
        XCTAssertEqual(p.x, 1, accuracy: 1e-9); XCTAssertEqual(p.y, 2, accuracy: 1e-9); XCTAssertEqual(p.z, 1, accuracy: 1e-9)
    }

    func testUnprojectOffCentreAndRotatedCamera() {
        // right 500 px and down 250 px at 2 m: 1 m right, 0.5 m down, 2 m ahead (-z)
        let p = CardMath.unproject(pixel: SIMD2(1460, 790), depth: 2, intrinsics: Self.k, camera: matrix_identity_float4x4)
        XCTAssertEqual(p.x, 1, accuracy: 1e-9); XCTAssertEqual(p.y, -0.5, accuracy: 1e-9); XCTAssertEqual(p.z, -2, accuracy: 1e-9)
        // camera turned 90 degrees about y (left): ahead is now -x
        var cam = simd_float4x4(simd_quatf(angle: .pi / 2, axis: SIMD3(0, 1, 0)))
        cam.columns.3 = SIMD4(1, 2, 3, 1)
        let q = CardMath.unproject(pixel: SIMD2(960, 540), depth: 2, intrinsics: Self.k, camera: cam)
        XCTAssertEqual(q.x, -1, accuracy: 1e-6); XCTAssertEqual(q.y, 2, accuracy: 1e-6); XCTAssertEqual(q.z, 3, accuracy: 1e-6)
    }

    func testIdFilterAcceptsOnlyTheThreeCardsWithNoBitErrors() {
        XCTAssertEqual(StationCard.accepting(id: 2, hamming: 0), .current)
        XCTAssertEqual(StationCard.accepting(id: 12, hamming: 0), .forward)
        XCTAssertEqual(StationCard.accepting(id: 26, hamming: 0), .rear)
        for id in [0, 1, 3, 5, 14, 15, 27, 29] { XCTAssertNil(StationCard.accepting(id: id, hamming: 0), "id \(id)") }
        XCTAssertNil(StationCard.accepting(id: 2, hamming: 1))
        XCTAssertEqual(StationCard.allCases.map(\.word), ["CURRENT", "FORWARD", "REAR"])
    }

    /// The image corners of a glyph of side `side` centred at `c` (OpenCV camera axes: x right, y down, z ahead),
    /// turned `yaw` radians about the camera's y axis.
    static func project(centre c: SIMD3<Double>, side: Double = StationCard.glyphSide, yaw: Double = 0) -> [SIMD2<Double>] {
        let h = side / 2
        return [SIMD2(-h, -h), SIMD2(h, -h), SIMD2(h, h), SIMD2(-h, h)].map { q in
            let p = c + SIMD3(q.x * cos(yaw), q.y, -q.x * sin(yaw))
            return SIMD2(1000 * p.x / p.z + 960, 1000 * p.y / p.z + 540)
        }
    }

    func testGlyphPoseFromPrintedSize() {
        for yaw in [0.0, 0.6, -1.0] {
            let px = Self.project(centre: SIMD3(0.3, -0.1, 2.5), yaw: yaw)
            guard let pose = CardMath.glyphPose(corners: px, side: StationCard.glyphSide, intrinsics: Self.k) else {
                return XCTFail("no pose at yaw \(yaw)")
            }
            // ARKit camera axes: y up, looking down -z
            XCTAssertEqual(pose.center.x, 0.3, accuracy: 1e-6); XCTAssertEqual(pose.center.y, 0.1, accuracy: 1e-6)
            XCTAssertEqual(pose.center.z, -2.5, accuracy: 1e-6)
            XCTAssertEqual(pose.corners.count, 4)
            XCTAssertEqual(simd_distance(pose.corners[0], pose.corners[1]), StationCard.glyphSide, accuracy: 1e-6)
        }
        // reversed corner order still gives the same centre
        let rev = Array(Self.project(centre: SIMD3(0, 0, 3)).reversed())
        XCTAssertEqual(CardMath.glyphPose(corners: rev, side: StationCard.glyphSide, intrinsics: Self.k)?.center.z ?? 0, -3, accuracy: 1e-6)
        XCTAssertNil(CardMath.glyphPose(corners: [SIMD2(1, 1), SIMD2(1, 1), SIMD2(1, 1), SIMD2(1, 1)], side: 0.0762, intrinsics: Self.k))
        let x = CardMath.solve([[2, 1], [1, 3]], [3, 5]) ?? []
        XCTAssertEqual(x.count, 2)
        XCTAssertEqual(x.first ?? 0, 0.8, accuracy: 1e-12); XCTAssertEqual(x.last ?? 0, 1.4, accuracy: 1e-12)
    }

    func testDepthUsesOnlyHighConfidence() {
        let v: [Float] = [1, 1, 1, 1, 9, 1, 1, 1, 1]
        let low = DepthGrid(width: 3, height: 3, values: v, confidence: [0, 1, 0, 1, 2, 1, 0, 0, 1])
        XCTAssertEqual(low.depth(atImagePixel: 150, 150, imageW: 300, imageH: 300) ?? 0, 9, accuracy: 1e-6)
        let none = DepthGrid(width: 3, height: 3, values: v, confidence: [UInt8](repeating: 1, count: 9))
        XCTAssertNil(none.depth(atImagePixel: 150, 150, imageW: 300, imageH: 300))
        var holes = [Float](repeating: 0, count: 9)
        holes[4] = 2.0; holes[3] = .nan; holes[5] = 2.2
        XCTAssertEqual(DepthGrid(width: 3, height: 3, values: holes).depth(atImagePixel: 150, 150, imageW: 300, imageH: 300) ?? 0,
                       2.2, accuracy: 1e-6)
    }

    func testSightingsCarrySightLineSizeAndLidar() {
        let px = Self.project(centre: SIMD3(0, 0, 2))
        let det = RawDetection(id: 12, hamming: 0, margin: 120, center: SIMD2(960, 540), corners: px)
        let stray = RawDetection(id: 5, hamming: 0, margin: 50, center: SIMD2(960, 540), corners: px)
        var cam = matrix_identity_float4x4
        cam.columns.3 = SIMD4(1, 0, 0, 1)
        let deep = DepthGrid(width: 4, height: 2, values: [Float](repeating: 3.7, count: 8), confidence: [UInt8](repeating: 2, count: 8))
        let r = CardMath.sightings(from: [det, stray], depth: deep, imageW: 1920, imageH: 1080, intrinsics: Self.k, camera: cam)
        XCTAssertEqual(r.found.count, 1)
        let s = r.found[0]
        XCTAssertEqual(s.card, .forward)
        XCTAssertEqual(s.eye, SIMD3(1, 0, 0))
        XCTAssertEqual(s.dir.z, -1, accuracy: 1e-9)
        XCTAssertEqual(s.sizeRange ?? 0, 2, accuracy: 1e-6)
        XCTAssertEqual(s.lidarRange ?? 0, 3.7, accuracy: 1e-6)
        // LiDAR says 3.7 m, the printed size 2 m: the bad depth is not believed
        XCTAssertEqual(s.range?.source, .size)
        XCTAssertEqual(s.point?.z ?? 0, -2, accuracy: 1e-6)
        XCTAssertEqual(s.corners.count, 4)
        let none = CardMath.sightings(from: [RawDetection(id: 2, hamming: 0, margin: 120, center: SIMD2(960, 540), corners: [])],
                                      depth: nil, imageW: 1920, imageH: 1080, intrinsics: Self.k, camera: cam)
        XCTAssertTrue(none.found.isEmpty)
        XCTAssertEqual(none.noRange, [.current])
    }

    func sighting(_ card: StationCard = .current, eye: SIMD3<Double> = .zero, dir: SIMD3<Double> = SIMD3(0, 0, -1),
                  size: Double? = 2, lidar: Double? = nil, mesh: Double? = nil, margin: Double = 30) -> CardSighting {
        var s = CardSighting(card: card, eye: eye, dir: dir, sizeRange: size, sizeCorners: [], lidarRange: lidar, margin: margin)
        s.meshRange = mesh
        return s
    }

    func testRangePreference() {
        XCTAssertEqual(sighting(lidar: 2.1, mesh: 1.98).range?.source, .mesh)
        XCTAssertEqual(sighting(lidar: 2.1, mesh: 3.5).range?.value, 2.1)        // mesh hole: through to the wall
        XCTAssertEqual(sighting(lidar: 3.69, mesh: 3.5).range?.source, .size)
        XCTAssertEqual(sighting(size: nil, lidar: 3.0, mesh: 2.5).range?.value, 2.5)
        XCTAssertEqual(sighting(size: nil, lidar: 3.0).range?.value, 3.0)
        XCTAssertNil(sighting(size: nil).range)
    }

    /// Two walls facing the camera, at 2 m and 4 m down -z.
    static func walls() -> MeshSnapshot {
        func wall(_ z: Float) -> MeshPart {
            MeshPart(positions: [SIMD3(-1, -1, z), SIMD3(1, -1, z), SIMD3(1, 1, z), SIMD3(-1, 1, z)],
                     normals: Array(repeating: SIMD3(0, 0, 1), count: 4), indices: [0, 1, 2, 0, 2, 3], transform: matrix_identity_float4x4)
        }
        return MeshSnapshot([wall(-2), wall(-4)])
    }

    func testSightLineMeetsMesh() {
        let m = Self.walls()
        let h = m.hits(origin: .zero, dir: SIMD3(0, 0, -1))
        XCTAssertEqual(h.count, 2)
        XCTAssertEqual(h[0], 2, accuracy: 1e-5); XCTAssertEqual(h[1], 4, accuracy: 1e-5)
        XCTAssertTrue(m.hits(origin: .zero, dir: SIMD3(0, 0, 1)).isEmpty)      // looking away
        XCTAssertTrue(m.hits(origin: .zero, dir: simd_normalize(SIMD3(1, 0, -0.2))).isEmpty)   // beside the walls
        XCTAssertEqual(m.range(for: sighting(size: nil)) ?? 0, 2, accuracy: 1e-5)  // first surface
        XCTAssertEqual(m.range(for: sighting(size: 3.8)) ?? 0, 4, accuracy: 1e-5)  // the one the size points at
        // anchor transforms are applied
        let moved = MeshSnapshot([MeshPart(positions: [SIMD3(-1, -1, 0), SIMD3(1, -1, 0), SIMD3(0, 1, 0)], normals: [],
                                           indices: [0, 1, 2], transform: simd_float4x4(SIMD4(1, 0, 0, 0), SIMD4(0, 1, 0, 0), SIMD4(0, 0, 1, 0), SIMD4(0, 0, -3, 1)))])
        XCTAssertEqual(moved.hits(origin: .zero, dir: SIMD3(0, 0, -1)).first ?? 0, 3, accuracy: 1e-5)
    }

    /// The cluttered-desk shot: clutter read as FORWARD with a decision margin under 1; real cards read 77-174.
    func testWeakReadsAreNotCards() {
        let px = Self.project(centre: SIMD3(0, 0, 2))
        let weak = RawDetection(id: 12, hamming: 0, margin: 0.8, center: SIMD2(960, 540), corners: px)
        let r = CardMath.sightings(from: [weak], depth: nil, imageW: 1920, imageH: 1080, intrinsics: Self.k, camera: matrix_identity_float4x4)
        XCTAssertTrue(r.found.isEmpty); XCTAssertTrue(r.noRange.isEmpty)
        XCTAssertEqual(r.weak, [.forward])
        XCTAssertEqual(StationCard.minMargin, 50)
    }

    /// Two false reads close to the phone weigh a lot each, but the many real looks win.
    func testHeaviestGroupWinsOverAFewHeavyFalseReads() {
        var t = CardTracker()
        for _ in 0..<2 { t.add(sighting(eye: SIMD3(0, 0, -1), dir: SIMD3(1, 0, 0), size: 0.18)) }    // weight 4 each
        for i in 0..<12 { t.add(sighting(eye: SIMD3(0, 0, 3), size: 0.7 + 0.001 * Double(i))) }      // ~2 each, at z = 2.3
        let e = t.tracks[.current]!.estimate!
        XCTAssertEqual(e.point.z, 2.295, accuracy: 0.01)
        XCTAssertEqual(e.kept, 12); XCTAssertEqual(e.rejected, 2)
    }

    /// Each pass meets at most `limit` sightings with the mesh: never-met first, then the longest ago.
    func testMeshBatchRotates() {
        var t = CardTracker()
        for _ in 0..<5 { t.add(sighting()) }
        let b1 = t.meshBatch(limit: 3)
        XCTAssertEqual(b1.map(\.seq), [0, 1, 2])
        t.applyMesh(Dictionary(uniqueKeysWithValues: b1.map { ($0.seq, Optional(1.9)) }), pass: 1)
        let b2 = t.meshBatch(limit: 3)
        XCTAssertEqual(b2.map(\.seq), [3, 4, 0])
        t.applyMesh(Dictionary(uniqueKeysWithValues: b2.map { ($0.seq, Optional(1.9)) }), pass: 2)
        XCTAssertEqual(t.meshBatch(limit: 3).map(\.seq), [1, 2, 0])
        t.add(sighting())
        XCTAssertEqual(t.meshBatch(limit: 2).map(\.seq), [5, 1])
    }

    /// ARKit corrects its map by moving anchors: readings kept relative to the card's anchor move with it, so
    /// looks from before and after the correction agree.
    func testSightingsFollowTheirCardAnchor() {
        var t = CardTracker()
        t.add(sighting(size: 2))                                  // card at (0, 0, -2)
        var a = matrix_identity_double4x4
        a.columns.3 = SIMD4(0, 0, -2, 1)
        t.pin(.current, a)
        XCTAssertEqual(t.tracks[.current]!.estimate!.point.z, -2, accuracy: 1e-9)   // pinning moves nothing
        XCTAssertEqual(t.tracks[.current]!.estimate!.point.x, 0, accuracy: 1e-9)
        var b = a
        b.columns.3 = SIMD4(0.15, 0, -2, 1)
        XCTAssertEqual(t.moveAnchor(.current, b), 0.15, accuracy: 1e-9)
        XCTAssertEqual(t.tracks[.current]!.estimate!.point.x, 0.15, accuracy: 1e-9)
        // a look in the corrected map lands with the old one
        t.add(sighting(eye: SIMD3(0.15, 0, 0), size: 2))
        var e = t.tracks[.current]!.estimate!
        XCTAssertEqual(e.kept, 2); XCTAssertEqual(e.spread, 0, accuracy: 1e-9)
        // turning the anchor turns the sight lines about it: the card stays at the anchor
        let turn = simd_double4x4(simd_quatd(angle: .pi / 2, axis: SIMD3(0, 1, 0)))
        var c = turn
        c.columns.3 = SIMD4(0.15, 0, -2, 1)
        t.moveAnchor(.current, c)
        e = t.tracks[.current]!.estimate!
        XCTAssertEqual(e.point.x, 0.15, accuracy: 1e-9); XCTAssertEqual(e.point.z, -2, accuracy: 1e-9)
        XCTAssertEqual(t.tracks[.current]!.sightings[0].eye.x, 2.15, accuracy: 1e-9)
        XCTAssertNil(t.tracks[.rear]?.anchor)
        XCTAssertEqual(t.moveAnchor(.rear, c), 0)                 // no track: nothing to move
    }

    func testSizeGateTightensWithDistanceKnown() {
        XCTAssertTrue(CardSighting.agree(4.6, 4.46))      // the 4.6 m card in the retest: 3 %
        XCTAssertTrue(CardSighting.agree(1.15, 1.23))
        XCTAssertFalse(CardSighting.agree(0.81, 0.62))    // shiny-laminate LiDAR at 0.6 m
        XCTAssertFalse(CardSighting.agree(1.12, 1.0))     // at least 10 cm, else 8 %
        XCTAssertFalse(CardSighting.agree(4.8, 4.4))
    }

    func testNearReadingsOutweighFarOnes() {
        var t = CardTracker()
        // three close looks put the card at z = -2.00
        for _ in 0..<3 { t.add(sighting(size: 2.0)) }
        // four looks from 4.4 m away (walked off) land 10 cm further on
        let far = SIMD3<Double>(0, 0, 2.5)
        for _ in 0..<4 { t.add(sighting(eye: far, size: 4.6)) }
        let e = t.tracks[.current]!.estimate!
        XCTAssertEqual(e.kept, 7)
        XCTAssertEqual(e.point.z, -2.0, accuracy: 1e-9)      // a plain median would say -2.10
        XCTAssertEqual(weightedMedian([(1, 1), (5, 1), (9, 10)]), 9)
        XCTAssertEqual(weightedMedian([(1, 1), (2, 1), (3, 1)]), 2)
    }

    func testMeshNearCardsOnly() {
        func wall(_ z: Float) -> MeshPart {
            MeshPart(positions: [SIMD3(-1, -1, z), SIMD3(1, -1, z), SIMD3(1, 1, z), SIMD3(-1, 1, z)],
                     normals: [], indices: [0, 1, 2, 0, 2, 3], transform: matrix_identity_float4x4)
        }
        let m = MeshSnapshot([wall(-2), wall(-4)], near: [SIMD3(0, 0, -4.5)], radius: 1)
        XCTAssertEqual(m.parts.count, 1)
        XCTAssertEqual(m.hits(origin: .zero, dir: SIMD3(0, 0, -1)).first ?? 0, 4, accuracy: 1e-5)
        XCTAssertEqual(MeshSnapshot([wall(-2), wall(-4)], near: [], radius: 1).parts.count, 0)
        XCTAssertEqual(MeshSnapshot([wall(-2), wall(-4)]).parts.count, 2)
    }

    func testEstimateRejectsBadReadingsAndFollowsTheMesh() {
        var t = CardTracker()
        XCTAssertTrue(t.add(sighting(size: 2.0, lidar: 2.02, margin: 20)))
        XCTAssertFalse(t.add(sighting(size: 2.05, lidar: 2.0, margin: 40)))
        t.add(sighting(size: 1.97, lidar: 2.01))
        t.add(sighting(size: nil, lidar: 3.69))                         // a bad depth with no size to check it
        guard let e = t.tracks[.current]?.estimate else { return XCTFail("no estimate") }
        XCTAssertEqual(e.kept, 3); XCTAssertEqual(e.rejected, 1)
        XCTAssertEqual(e.point.z, -2.01, accuracy: 1e-9)
        XCTAssertEqual(e.fromLidar, 3)
        XCTAssertLessThan(e.spread, 0.02)
        // the scan improves: every line now meets it at 1.95 m and the card moves there
        let ranges = CardTracker.meshRanges(t.allSightings, MeshSnapshot([MeshPart(
            positions: [SIMD3(-1, -1, -1.95), SIMD3(1, -1, -1.95), SIMD3(0, 1, -1.95)], normals: [], indices: [0, 1, 2],
            transform: matrix_identity_float4x4)]))
        XCTAssertEqual(ranges.count, 4)
        t.applyMesh(ranges)
        let f = t.tracks[.current]!.estimate!
        XCTAssertEqual(f.point.z, -1.95, accuracy: 1e-5)
        XCTAssertEqual(f.fromMesh, 4); XCTAssertEqual(f.rejected, 0)
        t.addMissed(.rear)
        XCTAssertEqual(t.seen.map(\.track.card), [.current])
        XCTAssertEqual(t.records.first?.sightings, 4)
        XCTAssertEqual(t.records.first?.word, "CURRENT")
        XCTAssertTrue(t.logLines[0].hasPrefix("card 2 CURRENT at (0.00, 0.00, -1.95) dist 1.95-1.95 m, 4 sightings (mesh 4, lidar 0, size 0), 0 rejected, spread 0 mm"),
                      t.logLines[0])
    }

    func testMedianEvenCount() {
        XCTAssertEqual(medianOf([4, 1, 3, 2]), 2.5)
        XCTAssertEqual(medianOf([]), 0)
    }

    func testSettledAfterEightTightLooks() {
        var t = CardTracker()
        for _ in 0..<7 { t.add(sighting(size: 2)) }
        XCTAssertFalse(t.tracks[.current]!.settled)
        XCTAssertFalse(t.allSettled)
        t.add(sighting(size: 2))
        XCTAssertTrue(t.tracks[.current]!.settled)
        XCTAssertTrue(t.allSettled)
        XCTAssertTrue(t.meshBatch(limit: 10, skipSettled: true).isEmpty)
        XCTAssertEqual(t.meshBatch(limit: 10).count, 8)
        var loose = CardTracker()
        for i in 0..<8 { loose.add(sighting(size: 2 + (i % 2 == 0 ? 0.02 : -0.02))) }
        XCTAssertFalse(loose.tracks[.current]!.settled)
        XCTAssertFalse(CardTracker().allSettled)
    }

    func testSettledHasHysteresis() {
        func look(_ t: inout CardTracker, _ n: Int, _ d: Double) {
            // sizes ramp over +-2d about 2 m, so the typical distance from the card's place is about d
            for i in 0..<n {   // taken from both ends in turn, so the spread is wide from the first look
                let k = i % 2 == 0 ? i / 2 : n - 1 - i / 2
                t.add(sighting(size: 2 + 2 * d * (Double(k) / Double(max(n - 1, 1)) * 2 - 1)))
            }
        }
        var t = CardTracker()
        look(&t, 8, 0.005)
        XCTAssertTrue(t.tracks[.current]!.settled)
        for _ in 0..<5 { look(&t, 20, 0.011) }   // looks arrive a few at a time
        let mid = t.tracks[.current]!.estimate!
        XCTAssertGreaterThan(mid.spread, 0.01); XCTAssertLessThan(mid.spread, 0.015)
        XCTAssertTrue(t.tracks[.current]!.settled)               // about 1.1 cm: not settled fresh, but stays settled
        for _ in 0..<5 { look(&t, 60, 0.02) }
        XCTAssertGreaterThanOrEqual(t.tracks[.current]!.estimate!.spread, 0.015)
        XCTAssertFalse(t.tracks[.current]!.settled)
        var fresh = CardTracker()
        look(&fresh, 100, 0.012)
        XCTAssertFalse(fresh.tracks[.current]!.settled)
    }

    func testCardReadWithoutRangeEndsBackoff() {
        var t = CardTracker()
        for _ in 0..<8 { t.add(sighting(size: 2)) }
        XCTAssertTrue(t.allSettled)
        t.addMissed(.rear)
        XCTAssertFalse(t.allSettled)
    }
}

/// The vendored detector itself, on a synthetic tag, plus the recorder's pure helpers for this build.
final class CardDetectorTests: XCTestCase {
    /// A tag16h5 glyph of `id` drawn big on white, as 8-bit grey.
    static func tagImage(id: Int, size: Int = 600) -> [UInt8] {
        let fam = tag16h5_create()!
        defer { tag16h5_destroy(fam) }
        let tag = apriltag_to_image(fam, UInt32(id))!
        defer { image_u8_destroy(tag) }
        let tw = Int(tag.pointee.width), scale = (size / 2) / tw, margin = (size - tw * scale) / 2
        var px = [UInt8](repeating: 255, count: size * size)
        for y in 0..<(tw * scale) { for x in 0..<(tw * scale) {
            px[(y + margin) * size + x + margin] = tag.pointee.buf[(y / scale) * Int(tag.pointee.stride) + x / scale]
        } }
        return px
    }

    func testFindsEachCardGlyph() {
        let d = CardDetector()
        for card in StationCard.allCases {
            var px = Self.tagImage(id: card.rawValue)
            let found = d.detectGrey(&px, width: 600, height: 600)
            XCTAssertEqual(found.map(\.id), [card.rawValue], card.word)
            XCTAssertEqual(found.first?.hamming, 0)
            XCTAssertEqual(found.first?.corners.count, 4)
            XCTAssertEqual(found.first?.center.x ?? 0, 300, accuracy: 3)
        }
    }

    func testLumaCopyDropsRowPaddingAndDetects() {
        let size = 600
        let src = Self.tagImage(id: StationCard.forward.rawValue, size: size)
        var buf: CVPixelBuffer?
        XCTAssertEqual(CVPixelBufferCreate(nil, size, size, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange, nil, &buf),
                       kCVReturnSuccess)
        guard let buf else { return XCTFail("no buffer") }
        CVPixelBufferLockBaseAddress(buf, [])
        let base = CVPixelBufferGetBaseAddressOfPlane(buf, 0)!
        let stride = CVPixelBufferGetBytesPerRowOfPlane(buf, 0)
        for y in 0..<size {
            let row = base.advanced(by: y * stride).assumingMemoryBound(to: UInt8.self)
            for x in 0..<size { row[x] = src[y * size + x] }
            for x in size..<stride { row[x] = 0 }  // padding must not leak into the copy
        }
        CVPixelBufferUnlockBaseAddress(buf, [])
        guard var grey = GreyImage(lumaOf: buf) else { return XCTFail("no copy") }
        XCTAssertEqual(grey.width, size); XCTAssertEqual(grey.height, size)
        XCTAssertEqual(grey.pixels, src)
        let found = CardDetector().detectGrey(&grey.pixels, width: size, height: size)
        XCTAssertEqual(found.map(\.id), [StationCard.forward.rawValue])
    }

    /// The printable sheet: Letter pages, front and back per card, and each front reads as its own card at the
    /// printed size.
    func testCardSheetPrintsReadableTrueSizeGlyphs() throws {
        let data = CardSheet.pdf()
        let doc = try XCTUnwrap(CGPDFDocument(CGDataProvider(data: data as CFData)!))
        XCTAssertEqual(doc.numberOfPages, 6)
        let page = try XCTUnwrap(doc.page(at: 1))
        XCTAssertEqual(page.getBoxRect(.mediaBox).size, CardSheet.pageSize)
        // black square = 6 of the 8 cells = 3 in = 216 pt
        XCTAssertEqual(CardSheet.box.width * 6 / 8, 216, accuracy: 0.01)
        let d = CardDetector()
        for (i, card) in StationCard.allCases.enumerated() {
            let p = try XCTUnwrap(doc.page(at: 1 + i * 2))
            let scale: CGFloat = 2, w = Int(CardSheet.pageSize.width * scale), h = Int(CardSheet.pageSize.height * scale)
            var px = [UInt8](repeating: 255, count: w * h)
            px.withUnsafeMutableBytes { buf in
                let g = CGContext(data: buf.baseAddress, width: w, height: h, bitsPerComponent: 8, bytesPerRow: w,
                                  space: CGColorSpaceCreateDeviceGray(), bitmapInfo: CGImageAlphaInfo.none.rawValue)!
                g.setFillColor(gray: 1, alpha: 1); g.fill(CGRect(x: 0, y: 0, width: w, height: h))
                g.scaleBy(x: scale, y: scale)
                g.drawPDFPage(p)
            }
            let found = d.detectGrey(&px, width: w, height: h).filter { StationCard.accepting(id: $0.id, hamming: $0.hamming) != nil }
            XCTAssertEqual(found.map(\.id), [card.rawValue], card.word)
            // side of the detected square, in points: 216
            if let f = found.first {
                XCTAssertEqual(simd_distance(f.corners[0], f.corners[1]) / Double(scale), 216, accuracy: 2)
            }
        }
    }

    func testStrayGlyphIsFoundButNotACard() {
        let d = CardDetector()
        var px = Self.tagImage(id: 5)
        let found = d.detectGrey(&px, width: 600, height: 600)
        XCTAssertEqual(found.map(\.id), [5])
        let r = CardMath.sightings(from: found, depth: DepthGrid(width: 1, height: 1, values: [2]), imageW: 600, imageH: 600,
                                   intrinsics: CardTrackingTests.k, camera: matrix_identity_float4x4)
        XCTAssertTrue(r.found.isEmpty)
        XCTAssertTrue(r.noRange.isEmpty)
    }

    func testBestFormatPrefersPixelsThenLowerFrameRate() {
        XCTAssertNil(ShotRecorder.bestFormatIndex([]))
        XCTAssertEqual(ShotRecorder.bestFormatIndex([(1920, 1440, 60), (3840, 2160, 30), (1280, 720, 60)]), 1)
        XCTAssertEqual(ShotRecorder.bestFormatIndex([(3840, 2160, 60), (3840, 2160, 30)]), 1)
    }

    func testKeyframeDownscale() {
        XCTAssertTrue(ShotRecorder.keyframeSize(width: 3840, height: 2160, maxWidth: 1920) == (1920, 1080))
        XCTAssertTrue(ShotRecorder.keyframeSize(width: 1920, height: 1440, maxWidth: 1920) == (1920, 1440))
        XCTAssertTrue(ShotRecorder.keyframeSize(width: 1280, height: 720, maxWidth: 1920) == (1280, 720))
        let k = simd_float3x3(columns: (SIMD3(2800, 0, 0), SIMD3(0, 2800, 0), SIMD3(1920, 1080, 1)))
        let s = ShotRecorder.scaledIntrinsics(k, from: (3840, 2160), to: (1920, 1080))
        XCTAssertEqual(s.columns.0.x, 1400); XCTAssertEqual(s.columns.1.y, 1400)
        XCTAssertEqual(s.columns.2.x, 960); XCTAssertEqual(s.columns.2.y, 540); XCTAssertEqual(s.columns.2.z, 1)
    }

    @MainActor func testShotWritesItsCards() throws {
        let over = Station(id: "a", label: "A1", x: 100, y: 200, z: 50)
        var tracker = CardTracker()
        // the test mesh is 1 m ahead but the printed size says 2 m: the mesh is not believed
        tracker.add(CardSighting(card: .forward, eye: .zero, dir: SIMD3(0, 0, -1), sizeRange: 2, sizeCorners: [],
                                 lidarRange: nil, margin: 30))
        let job = ShotJob(caveId: "test-cave-cards", over: over, parts: [ShotRecorderTests.tri], keyframes: [], startCamera: .zero,
                          tracking: "normal", systemVersion: "26.0",
                          compass: CompassReading(trueNorth: true, accuracyDeg: 8, headingDeg: 12), cards: tracker)
        defer { try? FileManager.default.removeItem(at: ShotFiles.folder(cave: "test-cave-cards")) }
        guard case .success(let url) = job.write() else { return XCTFail("shot not written") }
        let data = try Data(contentsOf: url)
        let cards = try ShotRecorderTests.header(of: data)["cards"] as? [[String: Any]]
        XCTAssertEqual(cards?.first?["word"] as? String, "FORWARD")
        XCTAssertEqual(cards?.first?["sightings"] as? Int, 1)
        // placed by the core: 2 m north, 0.30 m above the station
        let s = ShotSummary(Core.summary(bundle: data, pair: Core.pair(over, over)))
        XCTAssertEqual(s.cards.count, 1)
        XCTAssertEqual(s.cards[0].card, .forward)
        XCTAssertEqual(s.cards[0].point.y, 2, accuracy: 1e-4); XCTAssertEqual(s.cards[0].point.z, 0.30, accuracy: 1e-4)
        let scene = ShotView.scene(points: [0, 1, 0], over: over, forward: nil, cards: s.cards)
        XCTAssertNotNil(scene.rootNode.childNode(withName: "card-12", recursively: false))
    }
}
