import Foundation
import simd

/// The three station cards (`cards/make_cards.py`): AprilTag tag16h5 glyphs.
enum StationCard: Int, CaseIterable, Sendable {
    case current = 2, forward = 12, rear = 26

    /// The printed glyph's black square, edge to edge, metres: the large cards, measured at exactly 3 in.
    /// The card's distance is worked out from this size as well as from LiDAR.
    static let glyphSide = 0.0762

    var word: String {
        switch self { case .current: "CURRENT"; case .forward: "FORWARD"; case .rear: "REAR" }
    }

    /// What the card is for, printed on its back.
    var note: String {
        switch self {
        case .current: "Put on the station you capture from."
        case .forward: "Put on the next station."
        case .rear: "Put on the previous station."
        }
    }

    /// A read below this decision margin is not a card: on a cluttered desk clutter decoded as FORWARD at
    /// margin 0.6-0.8, while real cards read 77-174.
    static let minMargin = 50.0

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
    /// ARKit's confidence per value (0 low, 1 medium, 2 high), or nil when not known. Only high counts:
    /// low-confidence depth (edges, shiny laminate, far surfaces) is what put cards metres off.
    let confidence: [UInt8]?

    init(width: Int, height: Int, values: [Float], confidence: [UInt8]? = nil) {
        self.width = width; self.height = height; self.values = values; self.confidence = confidence
    }

    /// Median of the usable values in the 3x3 block round the pixel (u, v) of an image `imageW` x `imageH`;
    /// nil when none is usable (LiDAR sees about 5 m and not every surface).
    func depth(atImagePixel u: Double, _ v: Double, imageW: Int, imageH: Int) -> Double? {
        guard width > 0, height > 0, values.count >= width * height, imageW > 0, imageH > 0 else { return nil }
        let conf = confidence.flatMap { $0.count >= width * height ? $0 : nil }
        let cx = Int((u * Double(width) / Double(imageW)).rounded(.down))
        let cy = Int((v * Double(height) / Double(imageH)).rounded(.down))
        var found: [Float] = []
        for y in (cy - 1)...(cy + 1) where y >= 0 && y < height {
            for x in (cx - 1)...(cx + 1) where x >= 0 && x < width {
                let i = y * width + x
                if let conf, conf[i] < 2 { continue }
                let d = values[i]
                if d.isFinite && d > 0.05 { found.append(d) }
            }
        }
        guard !found.isEmpty else { return nil }
        found.sort()
        return Double(found[found.count / 2])
    }
}

/// Where a card's distance came from.
enum RangeSource: Sendable { case mesh, lidar, size }

/// One accepted card detection: the sight line from the camera through the glyph's centre, and the distances
/// along it that this frame offers. The point itself is decided later, so a better mesh can move it.
struct CardSighting: Equatable, Sendable {
    let card: StationCard
    /// Camera position, capture frame.
    let eye: SIMD3<Double>
    /// Unit direction from the camera through the glyph's centre, capture frame.
    let dir: SIMD3<Double>
    /// Eye to card centre from the glyph's printed size; nil when the corners gave no pose.
    let sizeRange: Double?
    /// The four corners from the printed size (capture frame), or empty.
    let sizeCorners: [SIMD3<Double>]
    /// Eye to card centre from high-confidence LiDAR depth in this frame; nil when there was none.
    let lidarRange: Double?
    let margin: Double
    /// Eye to card centre along the sight line where it meets the scanned mesh; filled in (and refreshed)
    /// as the mesh improves.
    var meshRange: Double? = nil
    /// Order of arrival in the shot, so mesh results can be matched back.
    var seq: Int = 0
    /// The mesh pass that last set `meshRange`; nil when none has yet.
    var meshedPass: Int? = nil

    /// Two ranges agree when within 8 % (at least 10 cm): what the printed size can vouch for. At 4K a 3 in
    /// glyph is still about 50 px wide at 4.5 m, so its range holds to a few percent; the retest's 4.6 m card
    /// had size and mesh 3 % apart.
    static func agree(_ r: Double, _ size: Double) -> Bool { abs(r - size) <= max(0.10, 0.08 * size) }

    /// The best distance: the mesh, then LiDAR depth, each only when the printed size agrees with it; else the
    /// size alone. With no size, the mesh or the depth as they are.
    var range: (value: Double, source: RangeSource)? {
        if let s = sizeRange {
            if let m = meshRange, Self.agree(m, s) { return (m, .mesh) }
            if let l = lidarRange, Self.agree(l, s) { return (l, .lidar) }
            return (s, .size)
        }
        if let m = meshRange { return (m, .mesh) }
        if let l = lidarRange { return (l, .lidar) }
        return nil
    }

    var point: SIMD3<Double>? { range.map { eye + dir * $0.value } }

    /// The same sighting with its sight line and corners moved by `m` (a rigid transform); distances unchanged.
    func transformed(_ m: simd_double4x4) -> CardSighting {
        func p(_ v: SIMD3<Double>) -> SIMD3<Double> { let w = m * SIMD4(v, 1); return SIMD3(w.x, w.y, w.z) }
        let d = m * SIMD4(dir, 0)
        var s = CardSighting(card: card, eye: p(eye), dir: simd_normalize(SIMD3(d.x, d.y, d.z)), sizeRange: sizeRange,
                             sizeCorners: sizeCorners.map(p), lidarRange: lidarRange, margin: margin)
        s.meshRange = meshRange; s.seq = seq; s.meshedPass = meshedPass
        return s
    }

    /// The corners moved along their own sight lines to agree with `range`.
    var corners: [SIMD3<Double>] {
        guard let s = sizeRange, s > 0, let r = range?.value, sizeCorners.count == 4 else { return [] }
        return sizeCorners.map { eye + ($0 - eye) * (r / s) }
    }
}

enum CardMath {
    /// A pixel with its depth to the capture frame. ARKit's camera looks down -z with y up; image pixels have y
    /// down, and `intrinsics` (fx, fy, cx, cy) are in the same pixels. `depth` is metres along the optical axis.
    static func unproject(pixel: SIMD2<Double>, depth: Double, intrinsics k: simd_float3x3,
                          camera: simd_float4x4) -> SIMD3<Double> {
        let fx = Double(k.columns.0.x), fy = Double(k.columns.1.y)
        let cx = Double(k.columns.2.x), cy = Double(k.columns.2.y)
        let local = SIMD3<Double>((pixel.x - cx) / fx * depth, -(pixel.y - cy) / fy * depth, -depth)
        return toWorld(local, camera)
    }

    /// A point in ARKit camera axes (x right, y up, looking down -z) to the capture frame.
    static func toWorld(_ p: SIMD3<Double>, _ camera: simd_float4x4) -> SIMD3<Double> {
        let m = simd_double4x4(SIMD4<Double>(camera.columns.0), SIMD4<Double>(camera.columns.1),
                               SIMD4<Double>(camera.columns.2), SIMD4<Double>(camera.columns.3))
        let w = m * SIMD4(p, 1)
        return SIMD3(w.x, w.y, w.z)
    }

    static func eye(_ camera: simd_float4x4) -> SIMD3<Double> {
        SIMD3(Double(camera.columns.3.x), Double(camera.columns.3.y), Double(camera.columns.3.z))
    }

    /// The glyph's centre and corners in ARKit camera axes from its four image corners and printed side: the
    /// plane-to-image homography, scaled by the known size. Nil when the corners do not make a square in front.
    static func glyphPose(corners px: [SIMD2<Double>], side: Double, intrinsics k: simd_float3x3)
        -> (center: SIMD3<Double>, corners: [SIMD3<Double>])? {
        guard px.count == 4, side > 0 else { return nil }
        let fx = Double(k.columns.0.x), fy = Double(k.columns.1.y)
        let cx = Double(k.columns.2.x), cy = Double(k.columns.2.y)
        guard fx > 0, fy > 0 else { return nil }
        let h = side / 2
        let plane: [SIMD2<Double>] = [SIMD2(-h, -h), SIMD2(h, -h), SIMD2(h, h), SIMD2(-h, h)]
        // normalised image coordinates, OpenCV axes (x right, y down, z ahead)
        let img = px.map { SIMD2(($0.x - cx) / fx, ($0.y - cy) / fy) }
        var a = [[Double]](), b = [Double]()
        for i in 0..<4 {
            let (X, Y, x, y) = (plane[i].x, plane[i].y, img[i].x, img[i].y)
            a.append([X, Y, 1, 0, 0, 0, -X * x, -Y * x]); b.append(x)
            a.append([0, 0, 0, X, Y, 1, -X * y, -Y * y]); b.append(y)
        }
        guard let v = solve(a, b) else { return nil }
        let h1 = SIMD3(v[0], v[3], v[6]), h2 = SIMD3(v[1], v[4], v[7]), h3 = SIMD3(v[2], v[5], 1)
        let n = simd_length(h1) + simd_length(h2)
        guard n > 0 else { return nil }
        var s = 2 / n
        if h3.z * s < 0 { s = -s }
        let t = h3 * s
        guard t.z > 0.05, t.x.isFinite, t.y.isFinite, t.z.isFinite else { return nil }
        let flip = { (p: SIMD3<Double>) in SIMD3(p.x, -p.y, -p.z) }   // OpenCV to ARKit camera axes
        let corners = plane.map { flip((h1 * $0.x + h2 * $0.y + h3) * s) }
        return (flip(t), corners)
    }

    /// Solves a small square linear system by Gaussian elimination with partial pivoting; nil when singular.
    static func solve(_ a0: [[Double]], _ b0: [Double]) -> [Double]? {
        var a = a0, b = b0
        let n = b.count
        for c in 0..<n {
            guard let p = (c..<n).max(by: { abs(a[$0][c]) < abs(a[$1][c]) }), abs(a[p][c]) > 1e-12 else { return nil }
            a.swapAt(c, p); b.swapAt(c, p)
            for r in (c + 1)..<n {
                let f = a[r][c] / a[c][c]
                if f == 0 { continue }
                for k in c..<n { a[r][k] -= f * a[c][k] }
                b[r] -= f * b[c]
            }
        }
        var x = [Double](repeating: 0, count: n)
        for r in stride(from: n - 1, through: 0, by: -1) {
            var s = b[r]
            for k in (r + 1)..<n { s -= a[r][k] * x[k] }
            x[r] = s / a[r][r]
        }
        return x
    }

    /// Cards among the detections, as sight lines with their size and LiDAR ranges. `noRange` lists cards seen
    /// with neither (no usable corners and no confident depth); `weak` card ids read below `minMargin`.
    static func sightings(from dets: [RawDetection], depth: DepthGrid?, imageW: Int, imageH: Int,
                          intrinsics: simd_float3x3, camera: simd_float4x4)
        -> (found: [CardSighting], noRange: [StationCard], weak: [StationCard]) {
        var found: [CardSighting] = [], missed: [StationCard] = [], weak: [StationCard] = []
        let eye = eye(camera)
        for d in dets {
            guard let card = StationCard.accepting(id: d.id, hamming: d.hamming) else { continue }
            guard d.margin >= StationCard.minMargin else { weak.append(card); continue }
            let far = unproject(pixel: d.center, depth: 1, intrinsics: intrinsics, camera: camera)
            let dir = simd_normalize(far - eye)
            guard dir.x.isFinite, dir.y.isFinite, dir.z.isFinite else { continue }
            let pose = glyphPose(corners: d.corners, side: StationCard.glyphSide, intrinsics: intrinsics)
            let sizeRange = pose.map { simd_length($0.center) }
            let sizeCorners = pose.map { $0.corners.map { toWorld($0, camera) } } ?? []
            let lidarRange = depth?.depth(atImagePixel: d.center.x, d.center.y, imageW: imageW, imageH: imageH).map {
                simd_distance(unproject(pixel: d.center, depth: $0, intrinsics: intrinsics, camera: camera), eye)
            }
            guard sizeRange != nil || lidarRange != nil else { missed.append(card); continue }
            found.append(CardSighting(card: card, eye: eye, dir: dir, sizeRange: sizeRange, sizeCorners: sizeCorners,
                                      lidarRange: lidarRange, margin: d.margin))
        }
        return (found, missed, weak)
    }
}

/// The scanned mesh as world-space triangles with a box per anchor, for meeting sight lines.
struct MeshSnapshot: Sendable {
    struct Part: Sendable {
        let lo: SIMD3<Float>, hi: SIMD3<Float>
        let v: [SIMD3<Float>]
        let tri: [UInt32]
    }
    let parts: [Part]

    /// `near`: keep only the parts whose box comes within `radius` of one of these points (the cards as now
    /// placed), so a long walk's mesh does not slow every pass; nil keeps everything.
    init(_ meshes: [MeshPart], near: [SIMD3<Double>]? = nil, radius: Float = 1) {
        let points = near?.map { SIMD3<Float>($0) }
        parts = meshes.compactMap { m in
            guard !m.positions.isEmpty, m.indices.count >= 3 else { return nil }
            let v = m.positions.map { p -> SIMD3<Float> in let w = m.transform * SIMD4(p, 1); return SIMD3(w.x, w.y, w.z) }
            var lo = v[0], hi = v[0]
            for p in v { lo = simd_min(lo, p); hi = simd_max(hi, p) }
            if let points, !points.contains(where: { simd_distance(simd_clamp($0, lo, hi), $0) <= radius }) { return nil }
            return Part(lo: lo, hi: hi, v: v, tri: m.indices)
        }
    }

    /// Distances along the ray (origin, unit dir) where it meets the mesh, beyond `minT`, nearest first.
    func hits(origin o: SIMD3<Double>, dir d: SIMD3<Double>, minT: Double = 0.1) -> [Double] {
        let of = SIMD3<Float>(o), df = SIMD3<Float>(d)
        var out: [Double] = []
        for p in parts where Self.boxHit(of, df, p.lo, p.hi) {
            var i = 0
            while i + 2 < p.tri.count {
                let a = Int(p.tri[i]), b = Int(p.tri[i + 1]), c = Int(p.tri[i + 2])
                i += 3
                guard a < p.v.count, b < p.v.count, c < p.v.count else { continue }
                if let t = Self.triHit(of, df, p.v[a], p.v[b], p.v[c]), Double(t) > minT { out.append(Double(t)) }
            }
        }
        // a line along a shared edge meets both triangles: count that surface once
        var hits: [Double] = []
        for t in out.sorted() where hits.last.map({ t - $0 > 0.001 }) ?? true { hits.append(t) }
        return hits
    }

    /// The mesh range for a sighting: the hit nearest the printed-size range when there is one (a hole in the
    /// mesh in front of the card lets the line through to the wall behind; the size spots that), else the first.
    func range(for s: CardSighting) -> Double? {
        let h = hits(origin: s.eye, dir: s.dir)
        guard !h.isEmpty else { return nil }
        guard let size = s.sizeRange else { return h[0] }
        return h.min { abs($0 - size) < abs($1 - size) }
    }

    static func boxHit(_ o: SIMD3<Float>, _ d: SIMD3<Float>, _ lo: SIMD3<Float>, _ hi: SIMD3<Float>) -> Bool {
        var t0: Float = 0, t1: Float = .greatestFiniteMagnitude
        for k in 0..<3 {
            if abs(d[k]) < 1e-9 {
                if o[k] < lo[k] - 0.01 || o[k] > hi[k] + 0.01 { return false }
                continue
            }
            var a = (lo[k] - 0.01 - o[k]) / d[k], b = (hi[k] + 0.01 - o[k]) / d[k]
            if a > b { swap(&a, &b) }
            t0 = max(t0, a); t1 = min(t1, b)
            if t0 > t1 { return false }
        }
        return true
    }

    /// Möller–Trumbore: distance along the ray to the triangle, or nil.
    static func triHit(_ o: SIMD3<Float>, _ d: SIMD3<Float>, _ a: SIMD3<Float>, _ b: SIMD3<Float>, _ c: SIMD3<Float>) -> Float? {
        let e1 = b - a, e2 = c - a
        let p = simd_cross(d, e2)
        let det = simd_dot(e1, p)
        if abs(det) < 1e-12 { return nil }
        let inv = 1 / det
        let s = o - a
        let u = simd_dot(s, p) * inv
        if u < 0 || u > 1 { return nil }
        let q = simd_cross(s, e1)
        let v = simd_dot(d, q) * inv
        if v < 0 || u + v > 1 { return nil }
        let t = simd_dot(e2, q) * inv
        return t > 0 ? t : nil
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

/// The value where the running weight (in value order) first reaches half the total.
func weightedMedian(_ v: [(value: Double, weight: Double)]) -> Double {
    let s = v.sorted { $0.value < $1.value }
    let half = s.reduce(0) { $0 + $1.weight } / 2
    var run = 0.0
    for e in s { run += e.weight; if run >= half { return e.value } }
    return s.last?.value ?? 0
}

/// Per-axis weighted median of points.
func weightedMedianPoint(_ v: [(point: SIMD3<Double>, weight: Double)]) -> SIMD3<Double> {
    SIMD3(weightedMedian(v.map { ($0.point.x, $0.weight) }), weightedMedian(v.map { ($0.point.y, $0.weight) }),
          weightedMedian(v.map { ($0.point.z, $0.weight) }))
}

/// A card's place worked out from all its sightings as they stand now.
struct CardEstimate: Equatable, Sendable {
    let point: SIMD3<Double>
    /// Median per corner over the kept sightings; empty when none had a pose.
    let corners: [SIMD3<Double>]
    /// Sightings kept, and those thrown out for lying more than `CardEstimate.reject` from the rest.
    let kept: Int, rejected: Int
    /// Typical distance of a kept sighting from the point (median), metres.
    let spread: Double
    /// How many kept sightings took their distance from the mesh, LiDAR depth and the printed size alone.
    let fromMesh: Int, fromLidar: Int, fromSize: Int
    /// Nearest and farthest camera-to-card distance among the kept sightings.
    let nearest: Double, farthest: Double

    /// A sighting this far from the median of all of them is a bad reading, metres.
    static let reject = 0.25

    /// How much a sighting counts: one over its distance squared (the printed size's error grows with the
    /// square of the distance, depth's at least linearly), so close looks outweigh ones from across the room.
    static func weight(_ range: Double) -> Double { 1 / pow(max(range, 0.5), 2) }

    init?(_ sightings: [CardSighting]) {
        let placed = sightings.compactMap { s in s.range.flatMap { r in s.point.map { (s, r, $0) } } }
        guard !placed.isEmpty else { return nil }
        let w = placed.map { Self.weight($0.1.value) }
        // The group with the most weight wins: the sighting with the most weight within `reject` of it is the
        // seed (a median of everything can sit on a few heavy false reads and throw the real ones out).
        var best = 0, bestSupport = -1.0
        for i in placed.indices {
            var support = 0.0
            for j in placed.indices where simd_distance(placed[i].2, placed[j].2) <= Self.reject { support += w[j] }
            if support > bestSupport { bestSupport = support; best = i }
        }
        let seed = weightedMedianPoint(placed.indices.filter { simd_distance(placed[$0].2, placed[best].2) <= Self.reject }
            .map { (placed[$0].2, w[$0]) })
        var keep = placed.filter { simd_distance($0.2, seed) <= Self.reject }
        if keep.isEmpty { keep = placed }
        let m = weightedMedianPoint(keep.map { ($0.2, Self.weight($0.1.value)) })
        point = m
        kept = keep.count; rejected = placed.count - keep.count
        spread = medianOf(keep.map { simd_distance($0.2, m) })
        fromMesh = keep.filter { $0.1.source == .mesh }.count
        fromLidar = keep.filter { $0.1.source == .lidar }.count
        fromSize = keep.filter { $0.1.source == .size }.count
        let ranges = keep.map(\.1.value)
        nearest = ranges.min() ?? 0; farthest = ranges.max() ?? 0
        let withCorners = keep.map { $0.0.corners }.filter { $0.count == 4 }
        corners = withCorners.isEmpty ? [] : (0..<4).map { i in medianPoint(withCorners.map { $0[i] }) }
    }
}

/// One card's sightings over a shot.
struct CardTrack: Sendable {
    let card: StationCard
    /// The sightings in the card anchor's own frame (the capture frame until an anchor is pinned).
    private var local: [CardSighting] = []
    private var marginSum = 0.0
    /// Detections with no distance at all to place them.
    var noRange = 0
    /// Where the card's ARKit anchor is now (capture frame from anchor); nil until one is pinned. ARKit moves
    /// anchors when it corrects its map, and the sightings move with it.
    private(set) var anchor: simd_double4x4?

    init(card: StationCard) { self.card = card }

    /// The sightings in the capture frame as the map stands now.
    var sightings: [CardSighting] { anchor.map { a in local.map { $0.transformed(a) } } ?? local }
    var count: Int { local.count }
    var meanMargin: Double { local.isEmpty ? 0 : marginSum / Double(local.count) }
    var estimate: CardEstimate? { CardEstimate(sightings) }
    /// Enough close agreement that more looks will not move it: 8 kept sightings within 1 cm (typical).
    var settled: Bool { estimate.map { $0.kept >= 8 && $0.spread < 0.01 } ?? false }

    /// Adds a sighting made in the capture frame as the map stands now.
    mutating func add(_ s: CardSighting) {
        local.append(anchor.map { s.transformed($0.inverse) } ?? s)
        marginSum += s.margin
    }

    /// Ties the sightings to an anchor just placed at `m`, without moving them.
    mutating func pin(_ m: simd_double4x4) {
        let world = sightings
        anchor = m
        local = world.map { $0.transformed(m.inverse) }
    }

    /// ARKit moved the anchor to `m`: the sightings move with it. Returns how far its origin moved, metres.
    @discardableResult
    mutating func moveAnchor(_ m: simd_double4x4) -> Double {
        guard let old = anchor else { return 0 }
        anchor = m
        return simd_distance(SIMD3(old.columns.3.x, old.columns.3.y, old.columns.3.z), SIMD3(m.columns.3.x, m.columns.3.y, m.columns.3.z))
    }

    /// Takes the mesh ranges just worked out (by sighting `seq`); a sighting missing from `ranges` keeps its own.
    mutating func applyMesh(_ ranges: [Int: Double?], pass: Int) {
        for i in local.indices {
            if let r = ranges[local[i].seq] { local[i].meshRange = r; local[i].meshedPass = pass }
        }
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
    private var nextSeq = 0

    /// True when this is the card's first sighting.
    @discardableResult
    mutating func add(_ s: CardSighting) -> Bool {
        let first = (tracks[s.card]?.count ?? 0) == 0
        var s = s
        s.seq = nextSeq; nextSeq += 1
        tracks[s.card, default: CardTrack(card: s.card)].add(s)
        return first
    }

    mutating func addMissed(_ c: StationCard) { tracks[c, default: CardTrack(card: c)].noRange += 1 }

    /// Ties a card's sightings to its ARKit anchor, just placed at `m`.
    mutating func pin(_ c: StationCard, _ m: simd_double4x4) { tracks[c]?.pin(m) }

    /// ARKit moved a card's anchor to `m`; returns how far, metres (0 for a card with no track or anchor).
    @discardableResult
    mutating func moveAnchor(_ c: StationCard, _ m: simd_double4x4) -> Double { tracks[c]?.moveAnchor(m) ?? 0 }

    /// Every sighting so far, for meeting with the mesh.
    /// Every card seen so far is settled (false before any is seen).
    var allSettled: Bool { let s = seen; return !s.isEmpty && s.allSatisfy { $0.track.settled } }
    var allSightings: [CardSighting] { tracks.values.flatMap(\.sightings) }

    /// Mesh ranges by sighting `seq`, nil where the line met no mesh, from mesh pass `pass`.
    mutating func applyMesh(_ ranges: [Int: Double?], pass: Int = 0) {
        for k in tracks.keys { tracks[k]?.applyMesh(ranges, pass: pass) }
    }

    /// The sightings for the next mesh pass, at most `limit`: never met with the mesh first (oldest first),
    /// then those met longest ago. Keeps each pass short however long the walk.
    func meshBatch(limit: Int, skipSettled: Bool = false) -> [CardSighting] {
        let pool = skipSettled ? tracks.values.filter { !$0.settled }.flatMap(\.sightings) : allSightings
        return Array(pool.sorted { ($0.meshedPass ?? -1, $0.seq) < ($1.meshedPass ?? -1, $1.seq) }.prefix(limit))
    }

    /// Meets every sighting with this mesh (call off the main thread for a big mesh).
    static func meshRanges(_ sightings: [CardSighting], _ mesh: MeshSnapshot) -> [Int: Double?] {
        var out: [Int: Double?] = [:]
        for s in sightings { out[s.seq] = .some(mesh.range(for: s)) }
        return out
    }

    /// Where the cards are now placed, for keeping only the mesh near them.
    var points: [SIMD3<Double>] { seen.map(\.estimate.point) }

    /// Cards with a placed estimate, in id order.
    var seen: [(track: CardTrack, estimate: CardEstimate)] {
        tracks.values.compactMap { t in t.estimate.map { (t, $0) } }.sorted { $0.track.card.rawValue < $1.track.card.rawValue }
    }

    var records: [CardSeenRecord] {
        seen.map { t, e in
            CardSeenRecord(id: t.card.rawValue, word: t.card.word, point: [e.point.x, e.point.y, e.point.z],
                           corners: e.corners.map { [$0.x, $0.y, $0.z] }, sightings: e.kept, spreadM: e.spread)
        }
    }

    /// One log line per seen card: id, word, position (capture frame, metres), nearest and farthest distance,
    /// sightings kept and where their distance came from, rejected readings, typical spread in mm, mean
    /// detector margin, detections with no distance at all.
    var logLines: [String] {
        seen.map { t, e in
            String(format: "card %d %@ at (%.2f, %.2f, %.2f) dist %.2f-%.2f m, %d sightings (mesh %d, lidar %d, size %d), %d rejected, spread %.0f mm, margin %.1f, %d without range",
                   t.card.rawValue, t.card.word, e.point.x, e.point.y, e.point.z, e.nearest, e.farthest, e.kept,
                   e.fromMesh, e.fromLidar, e.fromSize, e.rejected, e.spread * 1000, t.meanMargin, t.noRange)
        }
    }
}
