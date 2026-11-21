import SceneKit
import SwiftUI

/// What the Rust core says about a shot (`cc_shot_summary`), as plain values.
struct ShotSummary: Equatable, Sendable {
    var vertices = 0, triangles = 0, keyframes = 0
    var placed = false
    var turnDeg: Double?
    var error: String?
    /// The station the shot was taken over, and the one its heading tap aimed at.
    var stationId: String?
    var targetStationId: String?
    /// The station's label as saved (for a waiting shot, as typed).
    var stationLabel: String?
    /// Taken over a typed label the cave did not have yet (empty station id): placed by its label once the
    /// cave carries it.
    var isWaiting = false
    /// A compass shot (placed from its station alone); false for the older heading-tap shots.
    var isCompass = false
    /// A compass shot's compass at alignment (heading not stored: only true/magnetic and accuracy).
    var compass: CompassReading?
    /// Station cards the scan saw, relative to the station over (survey frame, metres).
    var cards: [CardPlaced] = []
    /// What to show for the compass; nil for a tap shot.
    var compassLabel: String? { compass?.label }

    init(vertices: Int, triangles: Int, keyframes: Int, placed: Bool, turnDeg: Double?, error: String?,
         stationId: String?, targetStationId: String?, isCompass: Bool) {
        self.vertices = vertices; self.triangles = triangles; self.keyframes = keyframes
        self.placed = placed; self.turnDeg = turnDeg; self.error = error
        self.stationId = stationId; self.targetStationId = targetStationId; self.isCompass = isCompass
    }

    init(_ d: [String: Any]) {
        vertices = d["vertices"] as? Int ?? 0
        triangles = d["triangles"] as? Int ?? 0
        keyframes = d["keyframes"] as? Int ?? 0
        placed = d["placed"] as? Bool ?? false
        turnDeg = d["turn_deg"] as? Double
        error = d["error"] as? String
        stationId = d["station_id"] as? String
        targetStationId = d["target_station_id"] as? String
        stationLabel = d["station_label"] as? String
        isWaiting = d["waiting"] as? Bool ?? (stationId == "")
        isCompass = d["compass"] is [String: Any]
        cards = (d["cards"] as? [[String: Any]] ?? []).compactMap(CardPlaced.init)
        if let c = d["compass"] as? [String: Any] {
            compass = CompassReading(trueNorth: c["true_north"] as? Bool ?? false,
                                     accuracyDeg: (c["accuracy_deg"] as? NSNumber)?.doubleValue ?? -1, headingDeg: 0)
        }
    }

    /// The shot's own stations among the cave's: nil when the station it stood on is gone (or, for a waiting
    /// shot, when no station carries its label yet). `forward` is the tap target of an older shot (nil for
    /// compass shots, and when that target is gone).
    func stations(in all: [Station]) -> (over: Station, forward: Station?)? {
        if isWaiting { return StationLabel.find(stationLabel ?? "", in: all).map { ($0, nil) } }
        guard let o = all.first(where: { !$0.isWaiting && $0.id == stationId }) else { return nil }
        return (o, targetStationId.flatMap { t in all.first { $0.id == t } })
    }

    /// A waiting shot's typed label while no station carries it; nil otherwise.
    func waitingLabel(in all: [Station]) -> String? {
        isWaiting && stations(in: all) == nil ? (stationLabel ?? "") : nil
    }

    /// The pair to draw it with: its placement, or for a compass shot still waiting for its station, the
    /// station at the origin (the mesh around its own spot, turned to north). Nil when it cannot be drawn.
    func viewPair(in all: [Station]) -> [Double]? {
        if let p = pair(in: all) { return p }
        return isCompass && waitingLabel(in: all) != nil ? [0, 0, 0, 0, 0, 0] : nil
    }

    /// The six numbers to place it with: a compass shot gives its station twice (the core ignores the target);
    /// a tap shot needs its target. Nil when a needed station is gone.
    func pair(in all: [Station]) -> [Double]? {
        guard let found = stations(in: all) else { return nil }
        if isCompass { return Core.pair(found.over, found.over) }
        return found.forward.map { Core.pair(found.over, $0) }
    }
}

/// A card the shot saw, placed in the survey frame relative to its station.
struct CardPlaced: Equatable, Sendable {
    let card: StationCard
    let point: SIMD3<Float>
    let sightings: Int
    let spreadM: Double

    init(card: StationCard, point: SIMD3<Float>, sightings: Int, spreadM: Double) {
        self.card = card; self.point = point; self.sightings = sightings; self.spreadM = spreadM
    }

    init?(_ d: [String: Any]) {
        guard let id = (d["id"] as? NSNumber)?.intValue, let card = StationCard(rawValue: id),
              let p = d["point"] as? [NSNumber], p.count == 3 else { return nil }
        self.init(card: card, point: SIMD3(p[0].floatValue, p[1].floatValue, p[2].floatValue),
                  sightings: (d["sightings"] as? NSNumber)?.intValue ?? 0, spreadM: (d["spread_m"] as? NSNumber)?.doubleValue ?? 0)
    }
}

struct ShotView: View {
    let url: URL
    /// Stations to find the shot's own in (the cave's, or just the one it was captured over).
    let stations: [Station]
    @State private var summary: ShotSummary?
    @State private var over: Station?
    @State private var forward: Station?
    @State private var placeable = false
    @State private var scene: SCNScene?
    /// The typed label of a shot still waiting for its station (drawn around its own spot, not placed).
    @State private var waiting: String?

    var body: some View {
        VStack(spacing: 0) {
            if let waiting {
                Text("Waiting for station \(waiting): not placed in the cave yet. Shown around its own spot.")
                    .font(.footnote).foregroundStyle(.orange).multilineTextAlignment(.center).padding(8)
            }
            if let scene {
                OrbitView(scene: scene).frame(maxHeight: .infinity)
            } else if let summary {
                Spacer()
                Text(Self.problem(summary, placeable: placeable)).multilineTextAlignment(.center).padding()
                Spacer()
            } else {
                ProgressView().frame(maxHeight: .infinity)
            }
            if let summary {
                HStack {
                    Text("\(summary.vertices) points")
                    Text("\(summary.triangles) triangles")
                    Text("\(summary.keyframes) photos")
                    if !summary.isCompass, let t = summary.turnDeg { Text(String(format: "turned %.1f°", t)) }
                    if !summary.cards.isEmpty { Text(summary.cards.map(\.card.word.capitalized).joined(separator: ", ")) }
                    if let c = summary.compass {
                        Text(c.label).foregroundStyle(c.warning == nil ? Color.secondary : Color.orange)
                    }
                }.font(.caption).padding(8)
            }
        }
        .navigationTitle(waiting.map { "\($0) · waiting" } ?? over.map { "over \($0.label)" } ?? "Shot")
        .task {
            let url = url, stations = stations
            // Reading and placing a shot is heavy: off the main actor, then shown here.
            let (s, found, waitingLabel, ok, points) = await Task.detached {
                () -> (ShotSummary, (over: Station, forward: Station?)?, String?, Bool, [Float]) in
                guard let data = try? Data(contentsOf: url, options: .alwaysMapped) else {
                    return (ShotSummary(["error": "unreadable"]), nil, nil, false, [])
                }
                let head = ShotSummary(Core.summary(bundle: data, pair: nil))
                let found = head.stations(in: stations), waiting = head.waitingLabel(in: stations)
                guard head.error == nil, let p = head.viewPair(in: stations) else { return (head, found, waiting, false, []) }
                let s = ShotSummary(Core.summary(bundle: data, pair: p))
                return (s, found, waiting, true, s.placed ? Core.placedPositions(bundle: data, pair: p) : [])
            }.value
            summary = s
            // still waiting: its own spot is the origin, marked with the typed label
            over = found?.over ?? waitingLabel.map { Station.waiting(label: $0) }
            forward = found?.forward
            waiting = waitingLabel
            placeable = ok
            if let o = over, s.placed, !points.isEmpty { scene = Self.scene(points: points, over: o, forward: forward, cards: s.cards) }
        }
    }

    /// What to say when there is no view.
    static func problem(_ s: ShotSummary, placeable: Bool) -> String {
        if s.error != nil { return "This shot could not be read." }
        if !placeable {
            if s.isWaiting { return "Waiting for station \(s.stationLabel ?? ""): not placed in the cave yet." }
            return "A station this shot was taken from is gone from the cave."
        }
        if s.isCompass { return "This shot could not be placed." }
        return "Not placed: pick a forward station that is not straight above or below, or aim again."
    }

    /// Survey (x east, y north, z up) shown in SceneKit (y up): (x, z, -y).
    nonisolated static func sceneVertices(_ points: [Float]) -> [SCNVector3] {
        var verts: [SCNVector3] = []
        verts.reserveCapacity(points.count / 3)
        var i = 0
        while i + 2 < points.count { verts.append(SCNVector3(points[i], points[i + 2], -points[i + 1])); i += 3 }
        return verts
    }

    /// Points come relative to the station over, so the scene's origin is that station.
    static func scene(points: [Float], over: Station, forward: Station?, cards: [CardPlaced] = []) -> SCNScene {
        let s = SCNScene()
        let verts = sceneVertices(points)
        let src = SCNGeometrySource(vertices: verts)
        let el = SCNGeometryElement(indices: Array(0..<Int32(verts.count)), primitiveType: .point)
        el.pointSize = 2; el.minimumPointScreenSpaceRadius = 1; el.maximumPointScreenSpaceRadius = 3
        let g = SCNGeometry(sources: [src], elements: [el])
        g.firstMaterial?.diffuse.contents = UIColor.systemGray
        g.firstMaterial?.lightingModel = .constant
        s.rootNode.addChildNode(SCNNode(geometry: g))
        // red at the station; blue at an older shot's aimed station
        for (st, colour) in [(over, UIColor.systemRed)] + (forward.map { [($0, UIColor.systemBlue)] } ?? []) {
            let n = SCNNode(geometry: SCNSphere(radius: 0.1))
            n.geometry?.firstMaterial?.diffuse.contents = colour
            // relative to the station over, in Double first (survey coordinates can be large)
            n.position = SCNVector3(Float(st.x - over.x), Float(st.z - over.z), Float(-(st.y - over.y)))
            s.rootNode.addChildNode(n)
        }
        // cards the scan saw: a labelled ball at each, relative to the station like the room
        for c in cards {
            let n = SCNNode(geometry: SCNSphere(radius: 0.06))
            n.geometry?.firstMaterial?.diffuse.contents = c.card.colour
            n.geometry?.firstMaterial?.lightingModel = .constant
            n.position = SCNVector3(c.point.x, c.point.z, -c.point.y)
            n.name = "card-\(c.card.rawValue)"
            let text = SCNText(string: c.card.word, extrusionDepth: 0.1)
            text.font = UIFont.boldSystemFont(ofSize: 4)
            text.firstMaterial?.diffuse.contents = c.card.colour
            text.firstMaterial?.lightingModel = .constant
            let label = SCNNode(geometry: text)
            label.scale = SCNVector3(0.03, 0.03, 0.03)
            let (lo, hi) = label.boundingBox
            label.pivot = SCNMatrix4MakeTranslation((lo.x + hi.x) / 2, -lo.y + 1, 0)
            label.position = SCNVector3(0, 0.12, 0)
            let look = SCNBillboardConstraint(); look.freeAxes = .all
            label.constraints = [look]
            n.addChildNode(label)
            s.rootNode.addChildNode(n)
        }
        return s
    }
}
