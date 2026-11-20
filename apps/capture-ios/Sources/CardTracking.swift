import Foundation
import simd

/// The three station cards (`cards/make_cards.py`): AprilTag tag16h5 glyphs.
enum StationCard: Int, CaseIterable, Sendable {
    case current = 2, forward = 12, rear = 26

    var word: String {
        switch self { case .current: "CURRENT"; case .forward: "FORWARD"; case .rear: "REAR" }
    }

    /// Only these ids, read with no bit errors, are cards; anything else the detector finds is noise
    /// (the range test saw stray ids 5, 14, 15, 27).
    static func accepting(id: Int, hamming: Int) -> StationCard? { hamming == 0 ? StationCard(rawValue: id) : nil }
}

/// One tag as the detector reports it, in image pixels (x right, y down). Corners in the detector's order.
struct RawDetection: Equatable, Sendable {
    var id: Int
    var hamming: Int
    var margin: Double
    var center: SIMD2<Double>
    var corners: [SIMD2<Double>]
}

/// The depth map copied out of ARKit (metres along the optical axis), so the frame is not held.
struct DepthGrid: Sendable {
    let width: Int, height: Int
    let values: [Float]

    /// Median of the valid values in the 3x3 block round the pixel (u, v) of an image `imageW` x `imageH`;
    /// nil when none is usable (LiDAR sees about 5 m and not every surface).
    func depth(atImagePixel u: Double, _ v: Double, imageW: Int, imageH: Int) -> Double? {
        guard width > 0, height > 0, values.count >= width * height, imageW > 0, imageH > 0 else { return nil }
        let cx = Int((u * Double(width) / Double(imageW)).rounded(.down))
        let cy = Int((v * Double(height) / Double(imageH)).rounded(.down))
        var found: [Float] = []
        for y in (cy - 1)...(cy + 1) where y >= 0 && y < height {
            for x in (cx - 1)...(cx + 1) where x >= 0 && x < width {
                let d = values[y * width + x]
                if d.isFinite && d > 0.05 { found.append(d) }
            }
        }
        guard !found.isEmpty else { return nil }
        found.sort()
        return Double(found[found.count / 2])
    }
}

/// One accepted card detection placed in the capture frame.
struct CardSighting: Equatable, Sendable {
    let card: StationCard
    let center: SIMD3<Double>
    /// The four corners in 3D; empty unless depth covered all four.
    let corners: [SIMD3<Double>]
    /// Camera to card centre, metres.
    let distance: Double
    let margin: Double
}

enum CardMath {
    /// A pixel with its depth to the capture frame. ARKit's camera looks down -z with y up; image pixels have y
    /// down, and `intrinsics` (fx, fy, cx, cy) are in the same pixels. `depth` is metres along the optical axis.
    static func unproject(pixel: SIMD2<Double>, depth: Double, intrinsics k: simd_float3x3,
                          camera: simd_float4x4) -> SIMD3<Double> {
        let fx = Double(k.columns.0.x), fy = Double(k.columns.1.y)
        let cx = Double(k.columns.2.x), cy = Double(k.columns.2.y)
        let local = SIMD4<Double>((pixel.x - cx) / fx * depth, -(pixel.y - cy) / fy * depth, -depth, 1)
        let m = simd_double4x4(SIMD4<Double>(camera.columns.0), SIMD4<Double>(camera.columns.1),
                               SIMD4<Double>(camera.columns.2), SIMD4<Double>(camera.columns.3))
        let w = m * local
        return SIMD3(w.x, w.y, w.z)
    }

    /// Cards among the detections, placed with depth. `noDepth` lists cards seen but with no depth at the centre.
    static func sightings(from dets: [RawDetection], depth: DepthGrid?, imageW: Int, imageH: Int,
                          intrinsics: simd_float3x3, camera: simd_float4x4) -> (found: [CardSighting], noDepth: [StationCard]) {
        var found: [CardSighting] = [], missed: [StationCard] = []
        let eye = SIMD3<Double>(Double(camera.columns.3.x), Double(camera.columns.3.y), Double(camera.columns.3.z))
        for d in dets {
            guard let card = StationCard.accepting(id: d.id, hamming: d.hamming) else { continue }
            guard let grid = depth, let dc = grid.depth(atImagePixel: d.center.x, d.center.y, imageW: imageW, imageH: imageH) else {
                missed.append(card); continue
            }
            let c = unproject(pixel: d.center, depth: dc, intrinsics: intrinsics, camera: camera)
            var corners: [SIMD3<Double>] = []
            if d.corners.count == 4 {
                let cd = d.corners.map { grid.depth(atImagePixel: $0.x, $0.y, imageW: imageW, imageH: imageH) }
                if cd.allSatisfy({ $0 != nil }) {
                    corners = zip(d.corners, cd).map {
                        unproject(pixel: $0, depth: $1!, intrinsics: intrinsics, camera: camera)
                    }
                }
            }
            guard c.x.isFinite, c.y.isFinite, c.z.isFinite else { continue }
            found.append(CardSighting(card: card, center: c, corners: corners, distance: simd_distance(c, eye), margin: d.margin))
        }
        return (found, missed)
    }
}

/// Median of a list (mean of the middle two when even).
func medianOf(_ v: [Double]) -> Double {
    guard !v.isEmpty else { return 0 }
    let s = v.sorted()
    return s.count % 2 == 1 ? s[s.count / 2] : (s[s.count / 2 - 1] + s[s.count / 2]) / 2
}

func medianPoint(_ v: [SIMD3<Double>]) -> SIMD3<Double> {
    SIMD3(medianOf(v.map(\.x)), medianOf(v.map(\.y)), medianOf(v.map(\.z)))
}

/// One card's sightings over a shot.
struct CardTrack: Sendable {
    let card: StationCard
    private(set) var centers: [SIMD3<Double>] = []
    private var cornerSamples: [[SIMD3<Double>]] = [[], [], [], []]
    private(set) var nearest = Double.infinity, farthest = 0.0
    private var marginSum = 0.0
    /// Detections with no depth to place them.
    var noDepth = 0

    init(card: StationCard) { self.card = card }

    var count: Int { centers.count }
    var median: SIMD3<Double> { medianPoint(centers) }
    var meanMargin: Double { centers.isEmpty ? 0 : marginSum / Double(centers.count) }
    /// How far the centre wandered: the largest distance of any sighting from the median, metres.
    var spread: Double { let m = median; return centers.map { simd_distance($0, m) }.max() ?? 0 }
    /// Median per corner; empty when depth never covered all four.
    var corners: [SIMD3<Double>] { cornerSamples[0].isEmpty ? [] : cornerSamples.map(medianPoint) }

    mutating func add(_ s: CardSighting) {
        centers.append(s.center)
        nearest = min(nearest, s.distance); farthest = max(farthest, s.distance)
        marginSum += s.margin
        if s.corners.count == 4 { for i in 0..<4 { cornerSamples[i].append(s.corners[i]) } }
    }
}

/// What the header records for one card (`cave_capture::CardSeen`).
struct CardSeenRecord: Equatable, Sendable {
    let id: Int, word: String
    let point: [Double]
    let corners: [[Double]]
    let sightings: Int
    let spreadM: Double

    var json: [String: Any] {
        ["id": id, "word": word, "point": point, "corners": corners, "sightings": sightings, "spread_m": spreadM]
    }
}

/// Running record of every card seen in one shot.
struct CardTracker: Sendable {
    private(set) var tracks: [StationCard: CardTrack] = [:]

    /// True when this is the card's first sighting.
    @discardableResult
    mutating func add(_ s: CardSighting) -> Bool {
        let first = tracks[s.card] == nil
        tracks[s.card, default: CardTrack(card: s.card)].add(s)
        return first
    }

    mutating func addMissed(_ c: StationCard) { tracks[c, default: CardTrack(card: c)].noDepth += 1 }

    /// Cards with at least one placed sighting, in id order.
    var seen: [CardTrack] { tracks.values.filter { $0.count > 0 }.sorted { $0.card.rawValue < $1.card.rawValue } }

    var records: [CardSeenRecord] {
        seen.map { t in
            let m = t.median
            return CardSeenRecord(id: t.card.rawValue, word: t.card.word, point: [m.x, m.y, m.z],
                                  corners: t.corners.map { [$0.x, $0.y, $0.z] }, sightings: t.count, spreadM: t.spread)
        }
    }

    /// One log line per seen card: id, word, median position (capture frame, metres), nearest and farthest
    /// distance, sightings, spread in mm, mean detector margin, detections that had no depth.
    var logLines: [String] {
        seen.map { t in
            let m = t.median
            return String(format: "card %d %@ at (%.2f, %.2f, %.2f) dist %.2f-%.2f m, %d sightings, spread %.0f mm, margin %.1f, %d without depth",
                          t.card.rawValue, t.card.word, m.x, m.y, m.z, t.nearest, t.farthest, t.count, t.spread * 1000,
                          t.meanMargin, t.noDepth)
        }
    }
}
