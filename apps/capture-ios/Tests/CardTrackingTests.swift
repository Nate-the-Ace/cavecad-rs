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

    func testSightingsFilterAndDepth() {
        let grid = DepthGrid(width: 4, height: 2, values: [Float](repeating: 3, count: 8))
        let dets = [
            RawDetection(id: 12, hamming: 0, margin: 40, center: SIMD2(960, 540),
                         corners: [SIMD2(900, 480), SIMD2(1020, 480), SIMD2(1020, 600), SIMD2(900, 600)]),
            RawDetection(id: 5, hamming: 0, margin: 50, center: SIMD2(960, 540), corners: []),
            RawDetection(id: 2, hamming: 1, margin: 50, center: SIMD2(960, 540), corners: []),
        ]
        let r = CardMath.sightings(from: dets, depth: grid, imageW: 1920, imageH: 1080, intrinsics: Self.k, camera: matrix_identity_float4x4)
        XCTAssertEqual(r.found.count, 1)
        XCTAssertEqual(r.found[0].card, .forward)
        XCTAssertEqual(r.found[0].center.z, -3, accuracy: 1e-9)
        XCTAssertEqual(r.found[0].distance, 3, accuracy: 1e-9)
        XCTAssertEqual(r.found[0].corners.count, 4)
        XCTAssertTrue(r.noDepth.isEmpty)
        let none = CardMath.sightings(from: dets, depth: nil, imageW: 1920, imageH: 1080, intrinsics: Self.k, camera: matrix_identity_float4x4)
        XCTAssertTrue(none.found.isEmpty)
        XCTAssertEqual(none.noDepth, [.forward])
    }

    func testDepthGridSamplingIgnoresHolesAndNaN() {
        var v = [Float](repeating: 0, count: 9)
        v[4] = 2.0; v[3] = .nan; v[5] = 2.2
        let g = DepthGrid(width: 3, height: 3, values: v)
        XCTAssertEqual(g.depth(atImagePixel: 150, _: 150, imageW: 300, imageH: 300) ?? 0, 2.2, accuracy: 1e-6)  // median of [2.0, 2.2]
        XCTAssertNil(DepthGrid(width: 3, height: 3, values: [Float](repeating: 0, count: 9)).depth(atImagePixel: 150, _: 150, imageW: 300, imageH: 300))
    }

    func sighting(_ card: StationCard, _ p: SIMD3<Double>, d: Double = 3, margin: Double = 30, corners: Bool = false) -> CardSighting {
        CardSighting(card: card, center: p, corners: corners ? [p, p, p, p] : [], distance: d, margin: margin)
    }

    func testTrackerRunningMedianAndStats() {
        var t = CardTracker()
        XCTAssertTrue(t.add(sighting(.current, SIMD3(0, 0, -2), d: 2, margin: 20)))
        XCTAssertFalse(t.add(sighting(.current, SIMD3(0.01, 0, -2), d: 2.5, margin: 40)))
        t.add(sighting(.current, SIMD3(0.50, 0, -2), d: 4, margin: 30, corners: true))   // an outlier: the median holds
        let c = t.tracks[.current]!
        XCTAssertEqual(c.count, 3)
        XCTAssertEqual(c.median.x, 0.01, accuracy: 1e-12)
        XCTAssertEqual(c.nearest, 2); XCTAssertEqual(c.farthest, 4)
        XCTAssertEqual(c.meanMargin, 30, accuracy: 1e-12)
        XCTAssertEqual(c.spread, 0.49, accuracy: 1e-12)
        XCTAssertEqual(c.corners.count, 4)
        t.addMissed(.rear)
        XCTAssertEqual(t.seen.map(\.card), [.current])   // REAR has no placed sighting
        XCTAssertEqual(t.tracks[.rear]?.noDepth, 1)
        let r = t.records
        XCTAssertEqual(r.count, 1)
        XCTAssertEqual(r[0].id, 2); XCTAssertEqual(r[0].word, "CURRENT"); XCTAssertEqual(r[0].sightings, 3)
        XCTAssertEqual(r[0].corners.count, 4)
        XCTAssertEqual(t.logLines.count, 1)
        XCTAssertTrue(t.logLines[0].hasPrefix("card 2 CURRENT at (0.01, 0.00, -2.00) dist 2.00-4.00 m, 3 sightings, spread 490 mm"))
    }

    func testMedianEvenCount() {
        XCTAssertEqual(medianOf([4, 1, 3, 2]), 2.5)
        XCTAssertEqual(medianOf([]), 0)
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

    func testStrayGlyphIsFoundButNotACard() {
        let d = CardDetector()
        var px = Self.tagImage(id: 5)
        let found = d.detectGrey(&px, width: 600, height: 600)
        XCTAssertEqual(found.map(\.id), [5])
        let r = CardMath.sightings(from: found, depth: DepthGrid(width: 1, height: 1, values: [2]), imageW: 600, imageH: 600,
                                   intrinsics: CardTrackingTests.k, camera: matrix_identity_float4x4)
        XCTAssertTrue(r.found.isEmpty)
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
        tracker.add(CardSighting(card: .forward, center: SIMD3(0, 0, -2), corners: [], distance: 2, margin: 30))
        let job = ShotJob(caveId: "test-cave-cards", over: over, parts: [ShotRecorderTests.tri], keyframes: [], startCamera: .zero,
                          tracking: "normal", systemVersion: "26.0",
                          compass: CompassReading(trueNorth: true, accuracyDeg: 8, headingDeg: 12), cards: tracker.records)
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
