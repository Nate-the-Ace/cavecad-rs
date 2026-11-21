import SceneKit
import SwiftUI
import UIKit

/// Every saved shot of a cave in one view, each placed at its own station and all relative to one origin.
/// The pure parts live here so they can be tested; the scene is built off the main actor.
enum Overall {
    /// One placed shot: its place in the saved list (for its colour), its station, and its thinned points
    /// relative to that station (survey frame, metres).
    struct Part: Sendable {
        let index: Int
        let over: Station
        let points: [Float]
    }

    /// Where `station` is from `origin`, survey frame (x east, y north, z up). Subtracted in Double first:
    /// survey coordinates can be millions of metres, where Float keeps only about half a metre.
    static func offset(_ station: Station, from origin: Station) -> SIMD3<Float> {
        SIMD3(Float(station.x - origin.x), Float(station.y - origin.y), Float(station.z - origin.z))
    }

    /// The origin: the station of the first shot that could be placed (the list is newest first).
    static func origin(_ parts: [Part]) -> Station? { parts.min { $0.index < $1.index }?.over }

    /// Hand-picked colours that tell apart well on a dark view; after these, golden-angle hues.
    static let palette: [SIMD3<Float>] = [
        [0.95, 0.62, 0.10], [0.34, 0.71, 0.95], [0.10, 0.75, 0.50], [0.95, 0.89, 0.26], [0.85, 0.47, 0.70],
        [0.90, 0.35, 0.20], [0.30, 0.45, 0.95], [0.60, 0.40, 0.90], [0.45, 0.90, 0.80], [0.95, 0.60, 0.60],
    ]

    /// The colour of the shot at `index` in the saved list, as red, green, blue 0…1. Depends on the index alone.
    static func colour(_ index: Int) -> SIMD3<Float> {
        if index >= 0, index < palette.count { return palette[index] }
        let h = (Double(abs(index)) * 0.618_033_988_75).truncatingRemainder(dividingBy: 1)
        var r: CGFloat = 0, g: CGFloat = 0, b: CGFloat = 0, a: CGFloat = 0
        UIColor(hue: h, saturation: 0.65, brightness: 0.95, alpha: 1).getRed(&r, green: &g, blue: &b, alpha: &a)
        return SIMD3(Float(r), Float(g), Float(b))
    }

    /// The box around every part's points, relative to `origin` (survey frame); nil when there are none.
    static func bounds(_ parts: [Part], origin: Station) -> (min: SIMD3<Float>, max: SIMD3<Float>)? {
        var lo = SIMD3<Float>(repeating: .infinity), hi = SIMD3<Float>(repeating: -.infinity)
        for p in parts {
            guard let b = PointCloud.bounds(p.points) else { continue }
            let o = offset(p.over, from: origin)
            lo = simd_min(lo, b.min + o); hi = simd_max(hi, b.max + o)
        }
        return lo.x <= hi.x ? (lo, hi) : nil
    }

    /// The stations to mark: those within `margin` metres of the box (relative to `origin`), and always the
    /// shots' own. In the order given.
    static func marked(_ stations: [Station], origin: Station, min lo: SIMD3<Float>, max hi: SIMD3<Float>,
                       margin: Float = 5, always: Set<String> = []) -> [Station] {
        let a = lo - margin, b = hi + margin
        return stations.filter { s in
            if always.contains(s.id) { return true }
            let p = offset(s, from: origin)
            return all(p .>= a) && all(p .<= b)
        }
    }

    /// The whole scene: every part in its colour at its station, marked stations as small labelled balls.
    /// The scene's origin is `origin`.
    static func scene(parts: [Part], origin: Station, stations: [Station]) -> SCNScene {
        let s = SCNScene()
        for p in parts {
            let c = colour(p.index)
            let n = PointCloud.node(points: p.points, colour: UIColor(red: CGFloat(c.x), green: CGFloat(c.y), blue: CGFloat(c.z), alpha: 1))
            let o = offset(p.over, from: origin)
            n.position = SCNVector3(o.x, o.z, -o.y)
            n.name = "shot-\(p.index)"
            s.rootNode.addChildNode(n)
        }
        guard let box = bounds(parts, origin: origin) else { return s }
        for st in marked(stations, origin: origin, min: box.min, max: box.max, always: Set(parts.map(\.over.id))) {
            let o = offset(st, from: origin)
            let n = SCNNode(geometry: SCNSphere(radius: 0.08))
            n.geometry?.firstMaterial?.diffuse.contents = UIColor.white
            n.geometry?.firstMaterial?.lightingModel = .constant
            n.position = SCNVector3(o.x, o.z, -o.y)
            n.name = "station-\(st.id)"
            let text = SCNText(string: st.label, extrusionDepth: 0.1)
            text.font = UIFont.boldSystemFont(ofSize: 4)
            text.firstMaterial?.diffuse.contents = UIColor.white
            text.firstMaterial?.lightingModel = .constant
            let label = SCNNode(geometry: text)
            label.scale = SCNVector3(0.05, 0.05, 0.05)
            let (tlo, thi) = label.boundingBox
            label.pivot = SCNMatrix4MakeTranslation((tlo.x + thi.x) / 2, -tlo.y + 1, 0)
            label.position = SCNVector3(0, 0.12, 0)
            let look = SCNBillboardConstraint(); look.freeAxes = .all
            label.constraints = [look]
            n.addChildNode(label)
            s.rootNode.addChildNode(n)
        }
        return s
    }
}

/// Carries a scene made off the main actor to the view. Safe: nothing else holds the scene by then.
private final class SceneBox: @unchecked Sendable {
    let scene: SCNScene
    init(_ scene: SCNScene) { self.scene = scene }
}

/// Every saved shot of the cave in one orbit view.
struct OverallView: View {
    let shots: [SavedShot]
    /// The cave's stations, to mark those around the shots.
    let stations: [Station]
    @State private var done = 0
    @State private var placed = 0
    @State private var skipped = 0
    @State private var waiting = 0
    @State private var finished = false
    @State private var scene: SCNScene?

    var body: some View {
        VStack(spacing: 0) {
            if let scene {
                OrbitView(scene: scene).frame(maxHeight: .infinity)
            } else if finished {
                Spacer()
                Text("None of the shots could be placed.").multilineTextAlignment(.center).padding()
                Spacer()
            } else {
                Spacer()
                ProgressView(value: Double(done), total: Double(max(shots.count, 1))) {
                    Text("Loading shot \(min(done + 1, shots.count)) of \(shots.count)")
                }
                .padding(32)
                Spacer()
            }
            if finished {
                HStack {
                    Text("\(placed) shot\(placed == 1 ? "" : "s")")
                    if waiting > 0 { Text("\(waiting) waiting for station").foregroundStyle(.secondary) }
                    if skipped > 0 {
                        Text("\(skipped) skipped (unreadable or station gone)").foregroundStyle(.orange)
                    }
                }
                .font(.caption).padding(8)
            }
        }
        .navigationTitle("Overall")
        .task { await load() }
    }

    /// One shot at a time off the main actor, counting here; then the scene, also off the main actor.
    private func load() async {
        guard scene == nil, !finished else { return }
        var parts: [Overall.Part] = []
        var skip = 0, wait = 0
        for (i, s) in shots.enumerated() {
            if Task.isCancelled { return }
            if s.waitingLabel != nil {
                // not in the cave yet: no place among the others
                wait += 1
            } else if let over = s.over, let pair = s.pair {
                let url = s.url
                let points = await Task.detached(priority: .userInitiated) { () -> [Float] in
                    guard let d = try? Data(contentsOf: url, options: .alwaysMapped) else { return [] }
                    return Core.placedPointsThin(bundle: d, pair: pair)
                }.value
                if points.isEmpty { skip += 1 } else { parts.append(Overall.Part(index: i, over: over, points: points)) }
            } else {
                skip += 1
            }
            done = i + 1
        }
        let all = stations
        if let origin = Overall.origin(parts) {
            let box = await Task.detached(priority: .userInitiated) {
                SceneBox(Overall.scene(parts: parts, origin: origin, stations: all))
            }.value
            scene = box.scene
        }
        placed = parts.count
        skipped = skip
        waiting = wait
        finished = true
        Log.write("shots", "overall: \(parts.count) placed, \(wait) waiting for station, \(skip) skipped")
    }
}
