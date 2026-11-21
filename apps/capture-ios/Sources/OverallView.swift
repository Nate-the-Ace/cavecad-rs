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

    /// A floating chain (none of its stations in the cave yet): its parts with `over` at the chain's own origin
    /// (its first shot's station, (0,0,0)), and the stations to mark, also in the chain's frame.
    struct FloatingGroup: Sendable {
        let title: String
        var parts: [Part]
        var marks: [Station]
    }

    /// Floating chains moved to stand side by side, east of `box` (relative to `origin`), 5 m apart: each part's
    /// `over` and each mark becomes an absolute station position. With no box they start at the origin.
    static func besideBox(_ groups: [FloatingGroup], origin: Station, box: (min: SIMD3<Float>, max: SIMD3<Float>)?)
        -> [FloatingGroup] {
        var cursor = Double(box.map { $0.max.x + 5 } ?? 0)
        let midY = Double(box.map { ($0.min.y + $0.max.y) / 2 } ?? 0)
        return groups.map { g in
            let zero = Station(id: "", label: "", x: 0, y: 0, z: 0)
            guard let b = bounds(g.parts, origin: zero) else { return g }
            let dx = cursor - Double(b.min.x), dy = midY - Double(b.min.y + b.max.y) / 2
            cursor += Double(b.max.x - b.min.x) + 5
            let move = { (st: Station) in
                Station(id: st.id, label: st.label, x: origin.x + dx + st.x, y: origin.y + dy + st.y, z: origin.z + st.z)
            }
            return FloatingGroup(title: g.title, parts: g.parts.map { Part(index: $0.index, over: move($0.over), points: $0.points) },
                                 marks: g.marks.map(move))
        }
    }

    /// The whole scene: every part in its colour at its station, marked stations as small labelled balls.
    /// The scene's origin is `origin`.
    static func scene(parts: [Part], origin: Station, stations: [Station], always: Set<String> = []) -> SCNScene {
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
        for st in marked(stations, origin: origin, min: box.min, max: box.max, always: Set(parts.map(\.over.id)).union(always)) {
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
    @State private var floating = 0
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
                    if floating > 0 { Text("\(floating) in waiting chains (drawn beside)").foregroundStyle(.orange) }
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
        var groups: [Int: Overall.FloatingGroup] = [:]
        // stations the cave lacks but a chain placed: marked with their label
        var extra: [Station] = []
        for (i, s) in shots.enumerated() {
            if Task.isCancelled { return }
            if let c = s.chain, let pair = c.pair, c.placed || c.isFloating {
                let url = s.url
                let raw = await Task.detached(priority: .userInitiated) { () -> [Float] in
                    guard let d = try? Data(contentsOf: url, options: .alwaysMapped) else { return [] }
                    return Core.placedPointsThin(bundle: d, pair: pair)
                }.value
                let points = c.apply(raw)
                let spot = c.spot ?? .zero
                // its own station when the cave has it, else a mark where the chain put it
                let own = s.over ?? Station(id: "chain-\(i)", label: "\(c.label) (waiting)", x: pair[0], y: pair[1], z: pair[2])
                if points.isEmpty {
                    skip += 1
                } else if c.isFloating, let k = c.chain {
                    let mark = Station(id: "float-\(i)", label: c.label, x: spot.x, y: spot.y, z: spot.z)
                    var g = groups[k] ?? Overall.FloatingGroup(title: s.chainNote ?? "waiting chain", parts: [], marks: [])
                    g.parts.append(Overall.Part(index: i, over: Station(id: "", label: "", x: 0, y: 0, z: 0), points: points))
                    g.marks.append(mark)
                    groups[k] = g
                } else {
                    parts.append(Overall.Part(index: i, over: own, points: points))
                    if s.over == nil { extra.append(own) }
                }
            } else if s.waitingLabel != nil {
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
        // waiting chains stand beside the placed shots, each marked with its chain's name at its first shot
        let origin = Overall.origin(parts) ?? Station(id: "", label: "", x: 0, y: 0, z: 0)
        let box = Overall.bounds(parts, origin: origin)
        let floatGroups = Overall.besideBox(groups.keys.sorted().compactMap { groups[$0] }, origin: origin, box: box)
        var all = stations + extra
        var shown = parts
        for g in floatGroups {
            shown += g.parts
            all += g.marks.enumerated().map { j, m in
                j == 0 ? Station(id: m.id, label: "\(m.label) · \(g.title)", x: m.x, y: m.y, z: m.z) : m
            }
        }
        let always = Set(extra.map(\.id) + floatGroups.flatMap { $0.marks.map(\.id) })
        if !shown.isEmpty {
            let made = await Task.detached(priority: .userInitiated) { [all, shown] in
                SceneBox(Overall.scene(parts: shown, origin: origin, stations: all, always: always))
            }.value
            scene = made.scene
        }
        let nFloat = floatGroups.reduce(0) { $0 + $1.parts.count }
        placed = parts.count
        floating = nFloat
        skipped = skip
        waiting = wait
        finished = true
        Log.write("shots", "overall: \(parts.count) placed, \(nFloat) in \(floatGroups.count) waiting chains, \(wait) waiting for station, \(skip) skipped")
    }
}
