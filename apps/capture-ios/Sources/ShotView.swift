import SceneKit
import SwiftUI

/// What the Rust core says about a shot (`cc_shot_summary`), as plain values.
struct ShotSummary: Equatable, Sendable {
    var vertices = 0, triangles = 0, keyframes = 0
    var placed = false
    var turnDeg: Double?
    var error: String?

    init(vertices: Int, triangles: Int, keyframes: Int, placed: Bool, turnDeg: Double?, error: String?) {
        self.vertices = vertices; self.triangles = triangles; self.keyframes = keyframes
        self.placed = placed; self.turnDeg = turnDeg; self.error = error
    }

    init(_ d: [String: Any]) {
        vertices = d["vertices"] as? Int ?? 0
        triangles = d["triangles"] as? Int ?? 0
        keyframes = d["keyframes"] as? Int ?? 0
        placed = d["placed"] as? Bool ?? false
        turnDeg = d["turn_deg"] as? Double
        error = d["error"] as? String
    }
}

struct ShotView: View {
    let url: URL; let over: Station; let forward: Station
    @State private var summary: ShotSummary?
    @State private var scene: SCNScene?

    var body: some View {
        VStack(spacing: 0) {
            if let scene {
                SceneView(scene: scene, options: [.allowsCameraControl, .autoenablesDefaultLighting]).frame(maxHeight: .infinity)
            } else if let summary {
                Spacer()
                if summary.error != nil {
                    Text("This shot could not be read.").padding()
                } else if !summary.placed {
                    Text("Not placed: pick a forward station that is not straight above or below, or aim again.")
                        .multilineTextAlignment(.center).padding()
                }
                Spacer()
            } else {
                ProgressView().frame(maxHeight: .infinity)
            }
            if let summary {
                HStack {
                    Text("\(summary.vertices) points")
                    Text("\(summary.triangles) triangles")
                    Text("\(summary.keyframes) photos")
                    if let t = summary.turnDeg { Text(String(format: "turned %.1f°", t)) }
                }.font(.caption).padding(8)
            }
        }
        .navigationTitle(over.label)
        .task {
            let url = url, pair = Core.pair(over, forward)
            // Reading and placing a shot is heavy: off the main actor, then shown here.
            let (s, points) = await Task.detached { () -> (ShotSummary, [Float]) in
                guard let data = try? Data(contentsOf: url) else { return (ShotSummary(["error": "unreadable"]), []) }
                let s = ShotSummary(Core.summary(bundle: data, pair: pair))
                return (s, s.placed ? Core.placedPositions(bundle: data, pair: pair) : [])
            }.value
            summary = s
            if s.placed, !points.isEmpty { scene = Self.scene(points: points, over: over, forward: forward) }
        }
    }

    /// Survey (x east, y north, z up) shown in SceneKit (y up): (x, z, -y).
    nonisolated static func sceneVertices(_ points: [Float]) -> [SCNVector3] {
        var verts: [SCNVector3] = []
        verts.reserveCapacity(points.count / 3)
        var i = 0
        while i + 2 < points.count { verts.append(SCNVector3(points[i], points[i + 2], -points[i + 1])); i += 3 }
        return verts
    }

    static func scene(points: [Float], over: Station, forward: Station) -> SCNScene {
        let s = SCNScene()
        let verts = sceneVertices(points)
        let src = SCNGeometrySource(vertices: verts)
        let el = SCNGeometryElement(indices: Array(0..<Int32(verts.count)), primitiveType: .point)
        el.pointSize = 2; el.minimumPointScreenSpaceRadius = 1; el.maximumPointScreenSpaceRadius = 3
        let g = SCNGeometry(sources: [src], elements: [el])
        g.firstMaterial?.diffuse.contents = UIColor.systemGray
        g.firstMaterial?.lightingModel = .constant
        s.rootNode.addChildNode(SCNNode(geometry: g))
        for (st, colour) in [(over, UIColor.systemRed), (forward, UIColor.systemBlue)] {
            let n = SCNNode(geometry: SCNSphere(radius: 0.1))
            n.geometry?.firstMaterial?.diffuse.contents = colour
            n.position = SCNVector3(Float(st.x), Float(st.z), Float(-st.y))
            s.rootNode.addChildNode(n)
        }
        return s
    }
}
